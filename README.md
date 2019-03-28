# Filesystem Forklift

Filesystem Migration Tool

-------------------------

Filesystem Forklift is an open source tool for migrating NFS and CIFS shares.  The goal is to quickly move large shares over the network through multiple Virtual Machines to a destination Gluster quickly and with little error.  Large shares these days may be considered impossible to move due to fact that it may take months to move the share.  Filesystem Forklift is intended to radically decrease the time needed to move the share, so that even seemingly impossibly large shares can be migrated to new clusters.

-------------------------

## To Start Using Filesystem Forklift

### Configuration:
1. Create your configuration file, forklift.json. The tool takes json config information.  
- The database_url, lifetime, src_path, dest_path, workgroup, and rerun fields are optional.  
- Database_url will allow Filesystem Forklift to send log messages and updates to the specified Postgres database server. 
- TimescaleDB is the preferred Postgres server type. 
- Lifetime changes the timeout time of a node from the default of 5 seconds.  
- Source and Destination filepaths are also optional, defaulting to "/", or the root directory, if not included.
- Workgroup is optional for an NFS share, and can therefore be omitted.  
- Rerun determines whether the program will wait for all nodes to finish before determining whether to rerun the program or
not.  Otherwise the program will terminate on each node as soon as it finishes processing (you will need to manually rerun the program if a node dies).  
Fields for this file are:
```
{
    "nodes": [
        "yourip:port",
        "clusterip:port",
        ...
    ],
    "lifetime": some positive non-zero number,
    "src_server": "shareserver",
    "dest_server": "destinationserver",
    "src_share": "/src_share",
    "dest_share": "/destination_share",
    "system": "Nfs or Samba",
    "debug_level": "OFF, FATAL, ERROR, WARN, INFO, DEBUG, or ALL",
    "num_threads": number from [0-some reasonable number],
    "workgroup": "WORKGROUP",
    "src_path": "/ starting directory of src share",
    "dest_path": "/ starting directory of destination share",
    "database_url": "postgresql://postgres:meow@127.0.0.1:8080"
    "rerun": true,
}
```
### Dependencies
1. libsmbclient-dev
2. libnfs-dev

## Quick Start Guide
### NFS
1. Download and build any dependencies for the forklift (see above)
2. Configure your forklift.json file on every node in your cluster
Example:
```
{
    "nodes": [
        "127.0.0.1:8888",
        "clusterip:port",
        ...
    ],
    "src_server": "10.0.0.24",
    "dest_server": "192.88.88.88",
    "src_share": "/src_share",
    "dest_share": "/destination_share",
    "system": "Nfs",
    "debug_level": "OFF",
    "num_threads": 20,
    "src_path": "/",
    "dest_path": "/",
    "database_url": "postgresql://postgres:meow@127.0.0.1:8080"
}
```
Note: 
- You can omit src_path and dest_path unless you are starting from a subdirectory in either of the shares. 
- leave workgroup out, as it is not needed for NFS
- lifetime can be adjusted, default is 5 seconds
- database_url is optional, only include it if you want to log data to a database, it will slow down the processes.
3. Initialize the forklift.  On each node in your cluster, type 
```
./filesystem_forklift
```
(if you configured the forklift.json in /etc/forklift).  Otherwise, type 
```
./filesystem_forklift -c path_to_directory_containing_config_file 
```
---
### Samba/CIFS
1. Download and build any dependencies for the forklift (see above).
2. Configure your smb.conf file on both shares.  You will need to do the following:
- set the netbios name to the same name for both shares
- set vfs objects = acl_xattr
- set map acl inherit = yes
- set store dos attributes = yes
- optionally, set the workgroup as the same on both shares
-Do not forget to restart smb or smbd (Samba)
3. Configure your forklift.json file on every node in your cluster
Example:
```
{
    "nodes": [
        "127.0.0.1:8888",
        "clusterip:port",
        ...
    ],
    "src_server": "10.0.0.24",
    "dest_server": "192.88.88.88",
    "src_share": "/src_share",
    "dest_share": "/destination_share",
    "system": "Samba",
    "debug_level": "OFF",
    "num_threads": 20,
    "workgroup": MYWORKGROUP,
    "src_path": "/",
    "dest_path": "/",
    "database_url": "postgresql://postgres:meow@127.0.0.1:8080"
}
```
Note:
- if you are using a glusterfs share, sometimes you are unable to edit the root directory.  In this case, create a subdirectory in the share(s) and change the src_path and dest_path accordingly.  Ex: "/sub_dir/"
- src_path and/or dest_path do not need to be included if you are starting from root 
- lifetime can be adjusted, default is 5 seconds
- database_url is optional, only include it if you want to log data to a database, it will slow down the processes.
4. Initialize the forklift.  On each node in your cluster, type 
```
sudo ./filesystem_forklift 
```
or 
```
sudo ./filesystem_forklift -u "username" -p "password" (if you configured the forklift.json in /etc/forklift)
```
Otherwise, type 
```
sudo ./filesystem_forklift -c path_to_directory_containing_config_file
```
or 
```
sudo ./filesystem_forklift -c path_to_directory_containing_config_file -u "username" -p "password"
```
If you do not include either the -u or -p flags, the program will prompt you for your Samba username and password.
Note:
The username and password should be the same on both shares.
## Idiosyncracies of Samba (AKA why you should just use the NFS option if possible)
There are many, many reasons why Samba is difficult, and why it is not recommended to use this program with Samba. While this functionality does work, it is slow and more error-prone than NFS.  A list of various Samba difficulties, quirks, and reasons follows
#### No Multithreading
Samba does not support multithreading.  Samba is a very old protocol using the SMB (Server Message Block) protocol, and explicitly does not allow for multithreading.  In regards to forklift this is restricting since one of the methods of increasing processing output and speed is through multithreading, which cannot actually be done.
#### Permissions Are Weird
##### DOS Attributes
Since Samba is meant for WINDOWS CIFS shares, permissions do not follow Unix permission patterns.  While Samba uses the same 9 bits to set its DOS attributes, that is about all it shares in common with the Unix 'mode' attribute.
![Samba Vs Unix Attributes]
(File-Forklift/SambaPermissions.png)
Unlike Unix permissions, DOS permissions are as such: R (Read-Only), H (Hidden), S (System), and A (Archive).  In Windows, if a file has none of these attributes, it is given the N (Normal) attribute.  These values are mapped to the Unix permission bits, which can lead to a discrepancy between Unix permissions on two files with the same DOS attributes.  Read-Only is raised when OWNER permission bits raise both read and write, and GROUP and OTHER/WORLD do not have write permissions.  Archive is mapped to OWNER execute, System to GROUP execute, and Archive to the OTHER/WORLD execute bit.  Of course, this means that, a file with Archive permission only could be 523, 563, 531 etc. Therefore, when copying permissions from one file to another, it is entirely possible that the Unix permissions look different.  Even using the chmod command to try to keep the two Unix permissions the same is a hit or miss, considering how chmod works in Samba. To be more precise, depending on the attributes of your Samba config file, chmod might not work at all, especially once ACL attributes are thrown into the mix.

It is important to note that by default, Samba creates files with with 744 (Unix) permissions and directories with 755 permissions unless configured otherwise.  
##### NT ACLs


