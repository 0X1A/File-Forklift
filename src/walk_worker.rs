use crate::error::*;
use crate::filesystem::*;
use crate::filesystem_entry::Entry;
use crate::filesystem_ops::*;
use crate::progress_message::ProgressMessage;

use crossbeam::channel::Sender;
use std::path::{Path, PathBuf};

pub struct WalkWorker {
    entry_output: Sender<Option<Entry>>,
    progress_output: Sender<ProgressMessage>,
    source: PathBuf,
}

impl WalkWorker {
    pub fn new(
        source: &Path,
        entry_output: Sender<Option<Entry>>,
        progress_output: Sender<ProgressMessage>,
    ) -> WalkWorker {
        WalkWorker {
            entry_output,
            progress_output,
            source: source.to_path_buf(),
        }
    }

    pub fn t_walk(
        &self,
        root_path: &Path,
        path: &Path,
        src_context: &mut NetworkContext,
        dest_context: &mut NetworkContext,
    ) -> ForkliftResult<()> {
        rayon::scope(|spawner| {
            let (mut num_files, mut total_size) = (0, 0);
            let (this, parent) = (Path::new("."), Path::new(".."));
            let check: bool;
            let mut check_paths: Vec<PathBuf> = vec![];
            let check_path = self.get_check_path(&path, root_path)?;
            check = exist(&check_path, dest_context);
            let dir = src_context.opendir(&path)?;
            for entrytype in dir {
                let entry = match entrytype {
                    Ok(f) => f,
                    Err(e) => {
                        error!("Error, non-unicode character in file path");
                        return Err(e);
                    }
                };
                let file_path = entry.path();
                if file_path != this && file_path != parent {
                    let newpath = path.join(&file_path);
                    let meta = self.process_file(&newpath, src_context);
                    if let Some(meta) = meta {
                        num_files += 1;
                        total_size += meta.size();
                        self.progress_output.send(ProgressMessage::Todo {
                            num_files,
                            total_size: total_size as usize,
                        });
                        match entry.filetype() {
                            GenericFileType::Directory => {
                                println!("dir: {:?}", &newpath);
                                let rec_ctx = src_context.clone();
                                let drec_ctx = dest_context.clone();
                                spawner.spawn(|_| {
                                    let mut rec_ctx = rec_ctx;
                                    let mut drec_ctx = drec_ctx;
                                    let newpath = newpath;
                                    self.t_walk(&root_path, &newpath, &mut rec_ctx, &mut drec_ctx)
                                        .unwrap()
                                });
                            }
                            GenericFileType::File => {
                                println!("file: {:?}", &newpath);
                            }
                            GenericFileType::Link => {
                                println!("link: {:?}", &newpath);
                            }
                            GenericFileType::Other => {}
                        }
                        if check {
                            let check_path = check_path.join(&file_path);
                            check_paths.push(check_path);
                        }
                    }
                }
            }
            // check through dest files
            self.check_and_remove(
                (check, &mut check_paths),
                (root_path, &path, dest_context),
                (this, parent),
            )?;
            Ok(())
        })?;
        self.entry_output.send(None);
        Ok(())
    }

    fn walk_loop(
        &self,
        (num_files, total_size): (&mut u64, &mut i64),
        (this, parent, path, stack): (&Path, &Path, &Path, &mut Vec<PathBuf>),
        (check, check_path, check_paths): (bool, &Path, &mut Vec<PathBuf>),
        (dir, src_context): (DirectoryType, &mut NetworkContext),
    ) -> ForkliftResult<()> {
        for entrytype in dir {
            let entry = entrytype?;
            let file_path = entry.path();
            if file_path != this && file_path != parent {
                let newpath = path.join(&file_path);
                //file exists?
                let meta = self.process_file(&newpath, src_context);
                if let Some(meta) = meta {
                    *num_files += 1;
                    *total_size += meta.size();
                    match self.progress_output.send(ProgressMessage::Todo {
                        num_files: *num_files,
                        total_size: *total_size as usize,
                    }) {
                        Ok(_) => {}
                        Err(e) => {
                            return Err(ForkliftError::FSError(format!(
                                "Error: {:?}, unable to send progress",
                                e
                            )));
                        }
                    };
                    match entry.filetype() {
                        GenericFileType::Directory => {
                            println!("dir: {:?}", &newpath);
                            stack.push(newpath.clone());
                        }
                        GenericFileType::File => {
                            println!("file: {:?}", newpath);
                        }
                        GenericFileType::Link => {
                            println!("link: {:?}", newpath);
                        }
                        GenericFileType::Other => {}
                    }
                    if check {
                        let check_path = check_path.join(file_path);
                        check_paths.push(check_path);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn s_walk(
        &self,
        root_path: &Path,
        src_context: &mut NetworkContext,
        dest_context: &mut NetworkContext,
    ) -> ForkliftResult<()> {
        let (mut num_files, mut total_size) = (0, 0);
        let mut stack: Vec<PathBuf> = vec![self.source.clone()];
        let (this, parent) = (Path::new("."), Path::new(".."));
        loop {
            let check: bool;
            let mut check_paths: Vec<PathBuf> = vec![];
            match stack.pop() {
                Some(p) => {
                    let check_path = self.get_check_path(&p, root_path)?;
                    check = exist(&check_path, dest_context);
                    let dir = src_context.opendir(&p)?;
                    self.walk_loop(
                        (&mut num_files, &mut total_size),
                        (this, parent, &p, &mut stack),
                        (check, &check_path, &mut check_paths),
                        (dir, src_context),
                    )?;
                    // check through dest files
                    self.check_and_remove(
                        (check, &mut check_paths),
                        (root_path, &p, dest_context),
                        (this, parent),
                    )?;
                }
                None => {
                    match self.entry_output.send(None) {
                        Ok(_) => {}
                        Err(e) => {
                            return Err(ForkliftError::FSError(format!(
                                "Error: {:?}, unable to send end processing signal",
                                e
                            )));
                        }
                    };
                    break;
                }
            }
        }
        Ok(())
    }

    fn get_check_path(&self, source_path: &Path, root_path: &Path) -> ForkliftResult<PathBuf> {
        let rel_path = get_rel_path(&source_path, &self.source)?;
        Ok(root_path.join(rel_path))
    }

    fn check_and_remove(
        &self,
        (check, check_paths): (bool, &mut Vec<PathBuf>),
        (root_path, source_path, dest_context): (&Path, &Path, &mut NetworkContext),
        (this, parent): (&Path, &Path),
    ) -> ForkliftResult<()> {
        // check through dest files
        if check {
            let check_path = self.get_check_path(&source_path, root_path)?;
            let dir = dest_context.opendir(&check_path)?;
            for entrytype in dir {
                let entry = entrytype?;
                let file_path = entry.path();
                if file_path != this && file_path != parent {
                    let newpath = check_path.join(file_path);
                    if !contains_and_remove(check_paths, &newpath) {
                        match entry.filetype() {
                            GenericFileType::Directory => {
                                println!("call remove_dir: {:?}", &newpath);
                                remove_dir(&newpath, dest_context)?;
                            }
                            _ => {
                                println!("remove: {:?}", &newpath);
                                remove_extra(&newpath, dest_context)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn process_file(&self, entry: &Path, src_context: &mut NetworkContext) -> Option<Stat> {
        let src_entry = Entry::new(entry, src_context);
        let metadata = match src_entry.metadata() {
            Some(stat) => stat,
            None => {
                return None;
            }
        };
        match self.entry_output.send(Some(src_entry.clone())) {
            Ok(_) => {}
            Err(e) => {
                return Err(ForkliftError::FSError(format!(
                    "Error: {:?}, unable to send entry for processing",
                    e
                )));
            }
        };
        Some(metadata.clone())
    }
}

fn contains_and_remove(check_paths: &mut Vec<PathBuf>, check_path: &Path) -> bool {
    for (count, source_path) in check_paths.iter().enumerate() {
        if source_path == check_path {
            check_paths.remove(count);
            return true;
        }
    }
    false
}

fn remove_extra(path: &Path, dest_context: &mut NetworkContext) -> ForkliftResult<()> {
    dest_context.unlink(path)
}

fn remove_dir(path: &Path, dest_context: &mut NetworkContext) -> ForkliftResult<()> {
    let (this, parent) = (Path::new("."), Path::new(".."));
    let mut stack: Vec<PathBuf> = vec![path.clone().to_path_buf()];
    let mut remove_stack: Vec<PathBuf> = vec![path.clone().to_path_buf()];
    loop {
        match stack.pop() {
            Some(p) => {
                let dir = dest_context.opendir(&p)?;
                for entrytype in dir {
                    let entry = match entrytype {
                        Ok(e) => e,
                        Err(e) => {
                            return Err(e);
                        }
                    };
                    let file_path = entry.path();
                    if file_path != this && file_path != parent {
                        let newpath = p.join(&file_path);
                        println!("remove: {:?}", &newpath);
                        match entry.filetype() {
                            GenericFileType::Directory => {
                                stack.push(newpath.clone());
                                remove_stack.push(newpath);
                            }
                            GenericFileType::File => {
                                remove_extra(&newpath, dest_context)?;
                            }
                            GenericFileType::Link => {
                                remove_extra(&newpath, dest_context)?;
                            }
                            GenericFileType::Other => {}
                        }
                    }
                }
                // check through dest files
            }
            None => {
                break;
            }
        }
    }
    while !remove_stack.is_empty() {
        let dir = match remove_stack.pop() {
            Some(e) => e,
            None => {
                return Err(ForkliftError::FSError(
                    "remove stack should not be empty!".to_string(),
                ));
            }
        };
        dest_context.rmdir(&dir)?;
    }
    Ok(())
}
