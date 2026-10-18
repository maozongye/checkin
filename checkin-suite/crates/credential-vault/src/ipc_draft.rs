//! Tauri IPC 薄封装示例（**未编译进 lib**；仅文档性草稿）。
//!
//! 完整草案见：`/workspace/docs/credential-tauri-ipc-draft.md`
//!
//! ```ignore
//! #[tauri::command]
//! fn credential_store(state: State<VaultState>, args: IpcStoreArgs) -> Result<IpcStoreOk, IpcErr> { ... }
//!
//! #[tauri::command]
//! fn credential_delete(state: State<VaultState>, args: IpcDeleteArgs) -> Result<IpcDeleteOk, IpcErr> { ... }
//!
//! // resolve 不注册为 command — 仅 Rust 同进程 Adapter 调用 CredentialStore::resolve
//! ```

#![allow(dead_code)]

// Intentionally empty — keep this file as a pointer; do not `mod ipc_draft` from lib.rs.
