# Vaultra

FTP, FTPS and SFTP client for Windows.

Two panels, one local and one remote. Drag files from one side to the other and that's it. You can save your servers, import them from FileZilla and WinSCP, pause and resume transfers and sync folders.

Files can also leave the app: drag them out of either panel and drop them on the desktop, Explorer or any program that accepts files. Remote files are downloaded at the moment you drop them. Ctrl+C and Ctrl+V work everywhere: between the two panels, inside the same server (remote to remote), between two connections, and with Explorer in both directions.

For SFTP it uses the OpenSSH that ships with Windows, so your ssh keys and config work without any extra setup.

## Requirements

- Windows 10 or 11
- OpenSSH Client installed, if you want SFTP (Settings > Apps > Optional features)

## Running the project

You need Rust, Node.js and Yarn installed.

```
yarn install
yarn tauri dev
```

To build the installer:

```
yarn tauri build
```

## Tests

```
cd src-tauri
cargo test
```
