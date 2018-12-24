extern crate chrono;
extern crate libnfs;
extern crate nix;
extern crate smbc;

use self::chrono::*;
use self::libnfs::*;
use self::nix::fcntl::OFlag;
use self::nix::sys::stat::Mode;
use crate::error::ForkliftResult;
use smbc::*;

use std::path::Path;

#[derive(Clone)]
/// a generic wrapper for filesystem contexts
pub enum NetworkContext {
    Samba(Smbc),
    Nfs(Nfs),
}

impl FileSystem for NetworkContext {
    fn create(&mut self, path: &Path, flags: OFlag, mode: Mode) -> ForkliftResult<FileType> {
        match self {
            NetworkContext::Nfs(nfs) => {
                let file = nfs.create(path, flags, mode)?;
                Ok(FileType::Nfs(file))
            }
            NetworkContext::Samba(smbc) => {
                let file = smbc.create(path, mode)?;
                Ok(FileType::Samba(file))
            }
        }
    }
    /// Please note, that Samba's chmod is very peculiar, and may conditionally work
    /// or fail depending on the samba config file.  As such, it is recommended to
    /// use setxattr, since samba uses DOS permissions
    fn chmod(&self, path: &Path, mode: Mode) -> ForkliftResult<()> {
        match self {
            NetworkContext::Nfs(nfs) => {
                nfs.lchmod(path, mode)?;
            }
            NetworkContext::Samba(smbc) => {
                smbc.chmod(path, mode)?;
            }
        }
        Ok(())
    }
    fn stat(&self, path: &Path) -> ForkliftResult<Stat> {
        match self {
            NetworkContext::Nfs(nfile) => {
                let stat = nfile.lstat64(path)?;
                let atime = Timespec::new(stat.nfs_atime as i64, stat.nfs_atime_nsec as i64);
                let mtime = Timespec::new(stat.nfs_mtime as i64, stat.nfs_mtime_nsec as i64);
                let ctime = Timespec::new(stat.nfs_ctime as i64, stat.nfs_ctime_nsec as i64);
                let s = (
                    stat.nfs_dev,
                    stat.nfs_ino,
                    stat.nfs_mode as u32,
                    stat.nfs_nlink,
                    stat.nfs_uid as u32,
                    stat.nfs_gid as u32,
                    stat.nfs_rdev,
                    stat.nfs_size as i64,
                    stat.nfs_blksize as i64,
                    stat.nfs_blocks as i64,
                );
                Ok(Stat::new(s, atime, mtime, ctime))
            }
            NetworkContext::Samba(sfile) => {
                let stat = sfile.stat(path)?;
                let atime = Timespec::new(stat.st_atim.tv_sec as i64, stat.st_atim.tv_nsec as i64);
                let ctime = Timespec::new(stat.st_ctim.tv_sec as i64, stat.st_ctim.tv_nsec as i64);
                let mtime = Timespec::new(stat.st_mtim.tv_sec as i64, stat.st_mtim.tv_nsec as i64);
                let s = (
                    stat.st_dev as u64,
                    stat.st_ino as u64,
                    stat.st_mode as u32,
                    stat.st_nlink as u64,
                    stat.st_uid as u32,
                    stat.st_gid as u32,
                    stat.st_rdev as u64,
                    stat.st_size as i64,
                    stat.st_blksize as i64,
                    stat.st_blocks as i64,
                );
                Ok(Stat::new(s, atime, mtime, ctime))
            }
        }
    }
    fn mkdir(&self, path: &Path) -> ForkliftResult<()> {
        match self {
            NetworkContext::Nfs(nfs) => {
                nfs.mkdir(path)?;
            }
            NetworkContext::Samba(smbc) => {
                smbc.mkdir(path, Mode::S_IRWXU)?;
            }
        }
        Ok(())
    }
    ///
    /// Please note that neither Samba nor Nfs use mode in their open function (
    /// the option might exist, but does nothing.) the mode parameter exists should
    /// another Filesystem need to be implemented where it's open function uses mode.
    ///
    fn open(&mut self, path: &Path, flags: OFlag, mode: Mode) -> ForkliftResult<FileType> {
        match self {
            NetworkContext::Nfs(nfs) => {
                let file = nfs.open(path, flags)?;
                Ok(FileType::Nfs(file))
            }
            NetworkContext::Samba(smbc) => {
                let file = smbc.open(path, flags, mode)?;
                Ok(FileType::Samba(file))
            }
        }
    }
    fn opendir(&mut self, path: &Path) -> ForkliftResult<DirectoryType> {
        match self {
            NetworkContext::Nfs(nfs) => {
                let dir = nfs.opendir(path)?;
                Ok(DirectoryType::Nfs(dir))
            }
            NetworkContext::Samba(smbc) => {
                let dir = smbc.opendir(path)?;
                Ok(DirectoryType::Samba(dir))
            }
        }
    }
    fn rename(&self, oldpath: &Path, newpath: &Path) -> ForkliftResult<()> {
        match self {
            NetworkContext::Nfs(nfs) => {
                nfs.rename(oldpath, newpath)?;
            }
            NetworkContext::Samba(smbc) => {
                smbc.rename(oldpath, newpath)?;
            }
        }
        Ok(())
    }
    fn unlink(&self, path: &Path) -> ForkliftResult<()> {
        match self {
            NetworkContext::Nfs(nfs) => {
                nfs.unlink(path)?;
            }
            NetworkContext::Samba(smbc) => {
                smbc.unlink(path)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
/// a generic wrapper for File handles
pub enum FileType {
    Samba(SmbcFile),
    Nfs(NfsFile),
}

impl File for FileType {
    fn read(&self, count: u64, offset: u64) -> ForkliftResult<Vec<u8>> {
        match self {
            FileType::Nfs(nfile) => {
                let buf = nfile.pread(count, offset)?;
                Ok(buf)
            }
            FileType::Samba(sfile) => {
                sfile.lseek(offset as i64, 0)?;
                let buf = sfile.fread(count)?;
                Ok(buf)
            }
        }
    }
    /// @note: we can return a u64 when the actual write calls return i32 because
    /// any negative values are indicative of errors, so they are already handled
    fn write(&self, buf: &[u8], offset: u64) -> ForkliftResult<u64> {
        match self {
            FileType::Nfs(nfile) => {
                let bytes = nfile.pwrite(buf, offset)?;
                Ok(bytes as u64)
            }
            FileType::Samba(sfile) => {
                sfile.lseek(offset as i64, 0)?;
                let bytes = sfile.fwrite(buf)?;
                Ok(bytes as u64)
            }
        }
    }
    /// Please NOTE: Samba stat function's attributes only have certain attributes that are
    /// the same values as a Unix call:
    /// inode, size, nlink, atime, mtime, and ctime
    /// blksize is hardcoded, mode uses Dos Mode, so use getxattr,
    fn fstat(&self) -> ForkliftResult<Stat> {
        match self {
            FileType::Nfs(nfile) => {
                let stat = nfile.fstat64()?;
                let atime = Timespec::new(stat.nfs_atime as i64, stat.nfs_atime_nsec as i64);
                let mtime = Timespec::new(stat.nfs_mtime as i64, stat.nfs_mtime_nsec as i64);
                let ctime = Timespec::new(stat.nfs_ctime as i64, stat.nfs_ctime_nsec as i64);
                let s = (
                    stat.nfs_dev,
                    stat.nfs_ino,
                    stat.nfs_mode as u32,
                    stat.nfs_nlink,
                    stat.nfs_uid as u32,
                    stat.nfs_gid as u32,
                    stat.nfs_rdev,
                    stat.nfs_size as i64,
                    stat.nfs_blksize as i64,
                    stat.nfs_blocks as i64,
                );
                Ok(Stat::new(s, atime, mtime, ctime))
            }
            FileType::Samba(sfile) => {
                let stat = sfile.fstat()?;
                let atime = Timespec::new(stat.st_atim.tv_sec as i64, stat.st_atim.tv_nsec as i64);
                let ctime = Timespec::new(stat.st_ctim.tv_sec as i64, stat.st_ctim.tv_nsec as i64);
                let mtime = Timespec::new(stat.st_mtim.tv_sec as i64, stat.st_mtim.tv_nsec as i64);
                let s = (
                    stat.st_dev as u64,
                    stat.st_ino as u64,
                    stat.st_mode as u32,
                    stat.st_nlink as u64,
                    stat.st_uid as u32,
                    stat.st_gid as u32,
                    stat.st_rdev as u64,
                    stat.st_size as i64,
                    stat.st_blksize as i64,
                    stat.st_blocks as i64,
                );
                Ok(Stat::new(s, atime, mtime, ctime))
            }
        }
    }
    fn truncate(&self, size: u64) -> ForkliftResult<()> {
        match self {
            FileType::Nfs(nfile) => {
                nfile.ftruncate(size)?;
            }
            FileType::Samba(sfile) => {
                sfile.ftruncate(size as i64)?;
            }
        }
        Ok(())
    }
}

/// general trait describing a File
pub trait File {
    /// read some number of bytes starting at offset from the file
    fn read(&self, count: u64, offset: u64) -> ForkliftResult<Vec<u8>>;
    /// write something to the file starting at offset
    fn write(&self, buf: &[u8], offset: u64) -> ForkliftResult<u64>;
    /// get this file's metadata
    fn fstat(&self) -> ForkliftResult<Stat>;
    /// truncate the file to size
    fn truncate(&self, size: u64) -> ForkliftResult<()>;
}

#[derive(Clone)]
/// an enum to hold the Directory structs of some generic FileSystem
pub enum DirectoryType {
    Samba(SmbcDirectory),
    Nfs(NfsDirectory),
}

#[derive(Clone, Debug, Copy, PartialEq, PartialOrd)]
/// a generic struct to hold the time values of a struct
pub struct Timespec {
    /// number of seconds since the system's EPOCH
    tv_sec: i64,
    tv_nsec: i64,
}

impl Timespec {
    pub fn new(sec: i64, nsec: i64) -> Self {
        Timespec {
            tv_sec: sec,
            tv_nsec: nsec,
        }
    }

    pub fn num_hours(&self) -> i64 {
        self.num_seconds() / 3600
    }

    pub fn num_minutes(&self) -> i64 {
        self.num_seconds() / 60
    }

    pub fn num_seconds(&self) -> i64 {
        if self.tv_sec < 0 && self.tv_nsec > 0 {
            self.tv_sec + 1
        } else {
            self.tv_sec
        }
    }

    pub fn num_milliseconds(&self) -> i64 {
        self.num_microseconds() / 1000
    }

    pub fn num_microseconds(&self) -> i64 {
        let secs = self.num_seconds() * 1_000_000;
        let usecs = self.micros_mod_sec();
        secs + usecs
    }

    fn micros_mod_sec(&self) -> i64 {
        if self.tv_sec < 0 && self.tv_nsec > 0 {
            self.tv_sec - 1_000_000
        } else {
            self.tv_nsec
        }
    }

    /// print the time formatted
    pub fn print_timeval_secs(&self) {
        let time = self.num_seconds();
        let naive_datetime = NaiveDateTime::from_timestamp(time, 0);
        let datetime: DateTime<Utc> = DateTime::from_utc(naive_datetime, Utc);
        println!("{:?}", datetime);
    }
}

#[derive(Clone, Debug, Copy, PartialOrd, PartialEq)]
/// A general struct for stat
pub struct Stat {
    /// ID of device containing file
    st_dev: u64,
    /// inode number
    st_ino: u64,
    /// Protection (access permissions)
    st_mode: u32,
    /// Number of hard links
    st_nlink: u64,
    /// User ID of the owner
    st_uid: u32,
    /// Group ID of the owner
    st_gid: u32,
    /// Device ID if special file
    st_rdev: u64,
    /// total size in bytes
    st_size: i64,
    /// blocksize for file system I/O
    st_blksize: i64,
    /// number of 512B blocks allocated
    st_blocks: i64,
    /// time of last Access
    st_atime: Timespec,
    /// time of last modification
    st_mtime: Timespec,
    /// time of last status change
    st_ctime: Timespec,
}

impl Stat {
    pub fn new(
        stat: (u64, u64, u32, u64, u32, u32, u64, i64, i64, i64),
        atime: Timespec,
        mtime: Timespec,
        ctime: Timespec,
    ) -> Self {
        Stat {
            st_dev: stat.0,
            st_ino: stat.1,
            st_mode: stat.2,
            st_nlink: stat.3,
            st_uid: stat.4,
            st_gid: stat.5,
            st_rdev: stat.6,
            st_size: stat.7,
            st_blksize: stat.8,
            st_blocks: stat.9,
            st_atime: atime,
            st_mtime: mtime,
            st_ctime: ctime,
        }
    }
    /// return ID of device containing file
    pub fn dev(&self) -> u64 {
        self.st_dev
    }
    /// return inode number
    pub fn ino(&self) -> u64 {
        self.st_ino
    }
    /// return file Protection (access permissions)
    pub fn mode(&self) -> u32 {
        self.st_mode
    }
    /// return Number of hard links
    pub fn nlink(&self) -> u64 {
        self.st_nlink
    }
    /// return User ID of the owner
    pub fn uid(&self) -> u32 {
        self.st_uid
    }
    /// return Group ID of the owner
    pub fn gid(&self) -> u32 {
        self.st_gid
    }
    /// return Device ID if special file
    pub fn rdev(&self) -> u64 {
        self.st_rdev
    }
    /// return total size in bytes
    pub fn size(&self) -> i64 {
        self.st_size
    }
    /// return blocksize for file system I/O
    pub fn blksize(&self) -> i64 {
        self.st_blksize
    }
    /// return number of 512B blocks allocated
    pub fn blocks(&self) -> i64 {
        self.st_blocks
    }
    /// return time of last Access
    pub fn atime(&self) -> Timespec {
        self.st_atime
    }
    /// return time of last modification
    pub fn mtime(&self) -> Timespec {
        self.st_mtime
    }
    /// return time of last status change
    pub fn ctime(&self) -> Timespec {
        self.st_ctime
    }
}

/// General trait describing a Filesystem
pub trait FileSystem {
    /// create a new FileType with the File trait
    fn create(&mut self, path: &Path, flags: OFlag, mode: Mode) -> ForkliftResult<FileType>;
    /// change the permissions on a file/directory to mode
    fn chmod(&self, path: &Path, mode: Mode) -> ForkliftResult<()>;
    /// get the metadata of a file
    fn stat(&self, path: &Path) -> ForkliftResult<Stat>;
    /// make a new directory at path
    fn mkdir(&self, path: &Path) -> ForkliftResult<()>;
    /// open a file at path
    fn open(&mut self, path: &Path, flags: OFlag, mode: Mode) -> ForkliftResult<FileType>;
    /// open a directory at path
    fn opendir(&mut self, path: &Path) -> ForkliftResult<DirectoryType>;
    /// rename a file/directory
    fn rename(&self, oldpath: &Path, newpath: &Path) -> ForkliftResult<()>;
    /// unlink (remove) a file/directory
    fn unlink(&self, path: &Path) -> ForkliftResult<()>;
}