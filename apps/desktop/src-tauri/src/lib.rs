mod commands;
mod monitor;
mod switcher;
mod vault;

use tauri::Manager;
use vault::{build_vault, resolve_app_data_dir, VaultState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(commands::AppState::default())
        .setup(|app| {
            let app_data = resolve_app_data_dir(app)?;
            let vault_dir = app_data.join("credential-vault");
            std::fs::create_dir_all(vault_dir.join("blobs"))
                .map_err(|e| format!("create vault blobs dir: {e}"))?;
            std::fs::create_dir_all(vault_dir.join("backups"))
                .map_err(|e| format!("create vault backups dir: {e}"))?;
            std::fs::create_dir_all(vault_dir.join("switch_blobs"))
                .map_err(|e| format!("create vault switch_blobs dir: {e}"))?;

            let backend = build_vault(vault_dir.clone());
            let vault_state = VaultState::new(backend, vault_dir);
            let vault_arc = vault_state.backend.clone();
            app.manage(vault_state);

            // Switcher：DB + VaultSwitchBackupStore（真 put_switch_backup）
            let switcher = switcher::SwitcherState::open(&app_data, vault_arc.clone())
                .or_else(|e| {
                    eprintln!("switcher open app_data failed ({e}); using temp fallback");
                    switcher::SwitcherState::open_fallback_temp(vault_arc)
                })
                .map_err(|e| format!("init SwitcherState: {e}"))?;
            app.manage(switcher);

            // Monitor：monitor/monitor.db（空库 DEV seed）
            let monitor_state = monitor::MonitorState::open(&app_data)
                .map_err(|e| format!("init MonitorState: {e}"))?;
            app.manage(monitor_state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            // account / checkin / scheduler
            commands::account::account_list,
            commands::checkin::checkin_run_all,
            commands::scheduler::scheduler_get,
            commands::scheduler::scheduler_set,
            // switch（Switcher 维护）
            commands::switch::switch_list,
            commands::switch::switch_precheck,
            commands::switch::switch_confirm_exec,
            commands::switch::switch_rollback_retry,
            // monitor
            commands::monitor::monitor_get_dashboard,
            commands::monitor::monitor_refresh,
            // credential — 真 vault + meta mock
            commands::credential::credential_store,
            commands::credential::credential_delete,
            commands::credential_meta::credential_meta_store,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Buddy 加油站");
}
