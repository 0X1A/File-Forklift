//SyncStats
use crate::error::{ForkliftError, ForkliftResult};
use crate::filesystem::*;
use crate::filesystem_entry::Entry;
use crate::filesystem_ops::SyncOutcome;
use crate::input::Input;
use crate::postgres_logger::{send_mess, EndState};
use crate::progress_message::*;
use crate::progress_worker::*;
use crate::rsync_worker::*;
use crate::socket_node::*;
use crate::walk_worker::*;
use crate::LogMessage;

use crossbeam::channel;
use crossbeam::channel::{Receiver, Sender};
use log::*;
use rendezvous_hash::{DefaultNodeHasher, RendezvousNodes};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Default, Debug, Clone, Copy)]
/// Hold the total stats of all files synced
pub struct SyncStats {
    /// total number of files in the source
    pub num_files: u64,
    /// Total size of all files in the source
    pub tot_size: usize,
    /// Number of files transferred from source
    /// to dest (should match tot_files if no error)
    pub num_synced: u64,
    /// number of files that are up to date (and therefore
    /// need no copies or modification)
    pub up_to_date: u64,
    /// the total number of files that were copied
    pub copied: u64,
    /// number of symlinks created in dest
    pub symlink_created: u64,
    /// number of symlinks updated in dest
    pub symlink_updated: u64,
    /// number of symlinks skipped in dest
    pub symlink_skipped: u64,
    /// number of files for which the permissions were updated
    pub permissions_update: u64,
    /// the number of files where dest file contents were updated
    pub checksum_updated: u64,
    /// the number of directories where dest directory was created
    pub directory_created: u64,
    /// the number of directories where the dest directory permissions were updated
    pub directory_updated: u64,
}

impl SyncStats {
    /// create a new zeroed SyncStats
    pub fn new() -> SyncStats {
        SyncStats {
            num_files: 0,
            tot_size: 0,
            num_synced: 0,
            up_to_date: 0,
            copied: 0,
            symlink_created: 0,
            symlink_updated: 0,
            symlink_skipped: 0,
            permissions_update: 0,
            checksum_updated: 0,
            directory_created: 0,
            directory_updated: 0,
        }
    }
    /// Add a SyncOutcome to the stats
    pub fn add_outcome(&mut self, outcome: &SyncOutcome) {
        self.num_synced += 1;
        match outcome {
            SyncOutcome::FileCopied(..) => self.copied += 1,
            SyncOutcome::UpToDate => self.up_to_date += 1,
            SyncOutcome::SymlinkUpdated => self.symlink_updated += 1,
            SyncOutcome::SymlinkCreated => self.symlink_created += 1,
            SyncOutcome::SymlinkSkipped => self.symlink_skipped += 1,
            SyncOutcome::PermissionsUpdated => self.permissions_update += 1,
            SyncOutcome::ChecksumUpdated(..) => self.checksum_updated += 1,
            SyncOutcome::DirectoryUpdated => self.directory_updated += 1,
            SyncOutcome::DirectoryCreated => self.directory_created += 1,
        }
    }
    pub fn reset(&mut self) {
        self.num_files = 0;
        self.tot_size = 0;
        self.up_to_date = 0;
        self.checksum_updated = 0;
        self.directory_created = 0;
        self.directory_updated = 0;
        self.num_synced = 0;
        self.permissions_update = 0;
        self.symlink_created = 0;
        self.symlink_skipped = 0;
        self.symlink_updated = 0;
        self.copied = 0;
    }
}

/// Struct to build and run Rsync
pub struct Rsyncer {
    /// source root path
    source: PathBuf,
    /// destination root path,
    destination: PathBuf,
    /// share protocol to usize
    filesystem_type: FileSystemType,
    /// console ouput functions
    progress_info: Box<ProgressInfo + Send + Sync>,
    /// channel to send postgres logs
    log_output: Sender<LogMessage>,
}

impl Rsyncer {
    /// create a new Rsyncer
    pub fn new(
        source: PathBuf,
        destination: PathBuf,
        filesystem_type: FileSystemType,
        progress_info: Box<ProgressInfo + Send + Sync>,
        log_output: Sender<LogMessage>,
    ) -> Rsyncer {
        Rsyncer { source, destination, filesystem_type, progress_info, log_output }
    }

    /// create the rsync workers and store them along with their
    /// respective input channels
    pub fn create_syncers(
        &self,
        contexts: &[(ProtocolContext, ProtocolContext)],
        send_progress: &Sender<ProgressMessage>,
    ) -> (Vec<Sender<Option<Entry>>>, Vec<RsyncWorker>) {
        let mut send_handles: Vec<Sender<Option<Entry>>> = Vec::new();
        let mut syncers: Vec<RsyncWorker> = Vec::new();
        for (src_context, dest_context) in contexts {
            let (send_e, rec_e) = channel::unbounded();
            send_handles.push(send_e);
            let sync_progress = send_progress.clone();
            syncers.push(RsyncWorker::new(
                self.source.as_path(),
                self.destination.as_path(),
                src_context.clone(),
                dest_context.clone(),
                rec_e,
                sync_progress,
                self.log_output.clone(),
            ));
        }
        (send_handles, syncers)
    }

    /// create the Filesystem contexts and store them in vectors
    pub fn create_contexts(
        &self,
        config: &Input,
        username: &str,
        password: &str,
    ) -> ForkliftResult<Vec<(ProtocolContext, ProtocolContext)>> {
        let mut contexts: Vec<(ProtocolContext, ProtocolContext)> = Vec::new();
        let level = &config.debug_level;
        let workgroup = &config.workgroup;
        let smbc = init_samba(&workgroup, username, password, *level)?;
        for _ in 0..config.num_threads {
            match self.filesystem_type {
                FileSystemType::Samba => {
                    let (src_context, dest_context) = (
                        ProtocolContext::Samba(Box::new(smbc.clone())),
                        ProtocolContext::Samba(Box::new(smbc.clone())),
                    );
                    contexts.push((src_context, dest_context));
                }
                FileSystemType::Nfs => {
                    let (src_context, dest_context) = (
                        create_nfs_context(&config.src_server, &config.src_share, *level)?,
                        create_nfs_context(&config.dest_server, &config.dest_share, *level)?,
                    );
                    contexts.push((src_context, dest_context));
                }
            }
        }
        Ok(contexts)
    }

    /// run the rsync protocol
    pub fn sync(
        self,
        config: &Input,
        (username, password): (&str, &str),
        nodelist: Arc<Mutex<RendezvousNodes<SocketNode, DefaultNodeHasher>>>,
        current_node: SocketNode,
        is_rerun: Sender<EndState>,
        end_run: Receiver<EndState>,
    ) -> ForkliftResult<()> {
        let (num_threads, src_share, dest_share) =
            (config.num_threads, &config.src_share, &config.dest_share);
        let (send_prog, rec_prog) = channel::unbounded::<ProgressMessage>();
        let (send_prog_thread, copy_log_output) = (send_prog.clone(), self.log_output.clone());
        let (get_signal, restart_signal) = channel::unbounded::<EndState>();
        let contexts = self.create_contexts(config, username, password)?;
        //create workers
        let (send_handles, syncers) = self.create_syncers(&contexts, &send_prog);
        let (src_path, dest_path) = (self.source.as_path(), self.destination.as_path());
        let walk_worker = WalkWorker::new(
            src_path,
            dest_path,
            contexts,
            current_node,
            nodelist,
            send_handles,
            send_prog,
        );
        let progress_worker = ProgressWorker::new(
            src_share,
            dest_share,
            self.progress_info,
            rec_prog,
            is_rerun,
            end_run,
        );

        rayon::spawn(move || {
            progress_worker.start(&copy_log_output, &get_signal).expect("Progress Worker Failed");
        });
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads as usize)
            .breadth_first()
            .build()
            .expect("Unable to build ThreadPool");
        loop {
            if num_threads == 1 {
                walk_worker.s_walk()?;
                walk_worker.stop()?;
            }
            let rsyncers = syncers.clone();
            pool.install(|| {
                if num_threads > 1 {
                    if let Err(e) = walk_worker.t_walk(src_path, &pool) {
                        return Err(e);
                    }
                    walk_worker.stop()?;
                }
                rayon::scope(|spawner| {
                    for syncer in rsyncers {
                        spawner.spawn(|_| {
                            let input = syncer.input.clone();
                            if let Err(e) = syncer.start(&pool) {
                                let mess = ProgressMessage::SendError(e);
                                send_prog_thread.send(mess).expect("Unable to send progress");
                            };
                            debug!(
                                "Syncer Stopped, Thread {:?}, num left {:?}",
                                pool.current_thread_index(),
                                input.len()
                            );
                        });
                    }
                });
                if send_prog_thread.send(ProgressMessage::EndSync).is_err() {
                    return Err(ForkliftError::CrossbeamChannelError(
                        "Unable to send End signal to progress_worker".to_string(),
                    ));
                };
                Ok(())
            })?;
            match restart_signal.recv() {
                Ok(EndState::EndProgram) => break Ok(()),
                Ok(EndState::Rerun) => (),
                Err(_) => {
                    let mess = LogMessage::Error(ForkliftError::CrossbeamChannelError(
                        "Unable to get end/restart signal from heartbeat".to_string(),
                    ));
                    if send_mess(mess, &self.log_output.clone()).is_err() {
                        break Err(ForkliftError::CrossbeamChannelError(
                            "Unable to log error to postgres".to_string(),
                        ));
                    }
                }
            }
        }?;
        Ok(())
    }
}
