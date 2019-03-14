# Filesystem Forklift

Filesystem Migration Tool

-------------------------

Filesystem Forklift is an open source tool for migrating NFS and CIFS shares.  The goal is to quickly move large shares over the network through multiple Virtual Machines to a destination Gluster quickly and with little error.  Large shares these days may be considered impossible to move due to fact that it may take months to move the share.  Filesystem Forklift is intended to radically decrease the time needed to move the share, so that even seemingly impossibly large shares can be migrated to new clusters.

-------------------------

## To Start Using Filesystem Forklift

### Configuration:
1. Create your configuration file, forklift.json. The tool takes json config information.  The database_url, lifetime, src_path, dest_path, and workgroup fields are optional.  Database_url will allow Filesystem Forklift to send log messages and updates to the specified Postgres database server. TimescaleDB is the preferred Postgres server type. Lifetime changes the timeout time of a node from the default of 5 seconds.  Workgroup is optional in that it is not needed for an NFS share, and can therefore be omitted.  Source and Destination filepaths are also optional, defaulting to "/", or the root directory, if not included.
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
}
```
### Dependencies
1. libsmbclient-dev
2. libnfs-dev

## Quick Start Guide
### NFS
1. Download and build any dependencies for the forklift (see above).  Nanomsg-1.1.4 can be found here:
- https://github.com/nanomsg/nanomsg
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
- src_path and dest_path should be "/" unless you are starting from a subdirectory in either of the shares. 
- leave workgroup out, as it is not needed for NFS
- lifetime can be adjusted, default is 5 seconds
- database_url is optional, only include it if you want to log data to a database, it will slow down the processes.
- if you are configuring a file for adding a node to a cluster, only include two socket addresses in the nodes section,the socket address of the node to be added, and the socket address of some node in the running cluster
3. Initialize the forklift.  On each node in your cluster, type 
```
./filesystem_forklift
```
(if you configured the forklift.json in /etc/forklift).  Otherwise, type 
```
./filesystem_forklift -c path_to_directory_containing_config_file 
```
--
### Samba/CIFS
1. Download and build any dependencies for the forklift (see above).  Nanomsg-1.1.4 can be found here:
- https://github.com/nanomsg/nanomsg
2. Configure your smb.conf file on both shares.  You will need to do the following:
- set the netbios name to the same name for both shares
- set vfs objects = acl_xattr
- set map acl inherit = yes
- set store dos attributes = yes
- optionally, set the workgroup as the same on both shares
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
- lifetime can be adjusted, default is 5 seconds
- database_url is optional, only include it if you want to log data to a database, it will slow down the processes.
- if you are configuring a file for adding a node to a cluster, only include two socket addresses in the nodes section,the socket address of the node to be added, and the socket address of some node in the running cluster.
4. Initialize the forklift.  On each node in your cluster, type 
```
./filesystem_forklift 
```
or 
```
./filesystem_forklift -u "username" -p "password" (if you configured the forklift.json in /etc/forklift)
```
Otherwise, type 
```
./filesystem_forklift -c path_to_directory_containing_config_file
```
or 
```
./filesystem_forklift -c path_to_directory_containing_config_file -u "username" -p "password"
```
If you do not include either the -u or -p flags, the program will prompt you for your Samba username and password.
---
Note:
The username and password should be the same on both shares.
