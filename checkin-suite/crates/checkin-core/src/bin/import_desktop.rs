//! CLI helper: import desktop login file → MemoryBackend vault, print report JSON.
//!
//! ```text
//! cargo run -p checkin-core --bin import_desktop -- --platform workbuddy --path ./fixture.info
//! cargo run -p checkin-core --bin import_desktop -- --platform trae_cn --path ./storage.json --vault-dir /tmp/vault
//! cargo run -p checkin-core --bin import_desktop -- --platform both --vault-dir /tmp/vault
//! cargo run -p checkin-core --bin import_desktop -- --platform both --wb-path ./wb.info --trae-path ./storage.json --vault-dir /tmp/vault
//! ```
//!
//! Trae / oversized payloads require `--vault-dir` (Auth file-mode blobs).
//! Default backend is in-memory. Does **not** enable LiveHttp.
//! Never prints tokens / cipher plaintext.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use checkin_core::{
    import_both_defaults, import_both_files, import_trae_storage_file, import_workbuddy_info_file,
    resolve_cli_path, DesktopPaths, ImportBothReport, ImportReport,
};
use credential_vault::{CredentialStore, MemoryBackend};

fn usage() -> ! {
    eprintln!(
        "Usage:\n  import_desktop --platform <workbuddy|trae_cn|both> [options]\n\n\
         Options:\n\
           --path <file>        single-platform file (workbuddy|trae_cn)\n\
           --wb-path <file>     both: WorkBuddy .info (else DesktopPaths default)\n\
           --trae-path <file>   both: Trae storage.json (else DesktopPaths default)\n\
           --vault-dir <dir>    required for Trae / oversized (file-mode blobs)\n\
           --persist-memory-dump\n\n\
         Defaults: MemoryBackend; report JSON to stdout (no secrets).\n\
         `both` never aborts one side when the other fails."
    );
    std::process::exit(2);
}

fn main() -> ExitCode {
    let mut platform: Option<String> = None;
    let mut path: Option<PathBuf> = None;
    let mut wb_path: Option<PathBuf> = None;
    let mut trae_path: Option<PathBuf> = None;
    let mut vault_dir: Option<PathBuf> = None;
    let mut persist_dump = false;

    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--platform" | "-p" => {
                platform = Some(args.next().unwrap_or_else(|| usage()));
            }
            "--path" => {
                path = Some(PathBuf::from(args.next().unwrap_or_else(|| usage())));
            }
            "--wb-path" => {
                wb_path = Some(PathBuf::from(args.next().unwrap_or_else(|| usage())));
            }
            "--trae-path" => {
                trae_path = Some(PathBuf::from(args.next().unwrap_or_else(|| usage())));
            }
            "--vault-dir" => {
                vault_dir = Some(PathBuf::from(args.next().unwrap_or_else(|| usage())));
            }
            "--persist-memory-dump" => persist_dump = true,
            "--help" | "-h" => usage(),
            other => {
                eprintln!("unknown arg: {other}");
                usage();
            }
        }
    }

    let platform = match platform {
        Some(p) => p,
        None => usage(),
    };
    let paths = DesktopPaths::default();
    let needs_vault = matches!(
        platform.as_str(),
        "both" | "trae" | "trae_cn"
    );
    let store = if needs_vault {
        let dir = match &vault_dir {
            Some(d) => d.clone(),
            None => {
                eprintln!("error: --vault-dir is required for platform `{platform}` (Trae/oversized file-mode)");
                return ExitCode::from(2);
            }
        };
        let _ = std::fs::create_dir_all(&dir);
        CredentialStore::with_vault_dir(MemoryBackend::new(), dir)
    } else {
        CredentialStore::new(MemoryBackend::new())
    };

    let exit = match platform.as_str() {
        "both" => run_both(
            &store,
            &paths,
            wb_path.as_ref(),
            trae_path.as_ref(),
            path.as_ref(),
        ),
        "workbuddy" | "wb" | "trae" | "trae_cn" => {
            let resolved = match resolve_cli_path(&platform, path.as_deref(), &paths) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(1);
                }
            };
            let report: ImportReport = match platform.as_str() {
                "workbuddy" | "wb" => match import_workbuddy_info_file(&store, &resolved, None) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("import failed: {e}");
                        return ExitCode::from(1);
                    }
                },
                "trae" | "trae_cn" => {
                    match import_trae_storage_file(&store, &resolved, &paths, None) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("import failed: {e}");
                            return ExitCode::from(1);
                        }
                    }
                }
                _ => unreachable!(),
            };
            match serde_json::to_string_pretty(&report) {
                Ok(s) => {
                    println!("{s}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("serialize report: {e}");
                    ExitCode::from(1)
                }
            }
        }
        other => {
            eprintln!("unknown platform `{other}` (workbuddy|trae_cn|both)");
            ExitCode::from(2)
        }
    };

    if persist_dump {
        eprintln!(
            "persist-memory-dump: memory_entries={}",
            store.backend().len()
        );
    }

    exit
}

fn run_both(
    store: &CredentialStore<MemoryBackend>,
    paths: &DesktopPaths,
    wb_path: Option<&PathBuf>,
    trae_path: Option<&PathBuf>,
    legacy_path: Option<&PathBuf>,
) -> ExitCode {
    if legacy_path.is_some() && wb_path.is_none() && trae_path.is_none() {
        eprintln!("for --platform both use --wb-path / --trae-path (or omit for DesktopPaths defaults)");
        return ExitCode::from(2);
    }

    let both: ImportBothReport = if wb_path.is_none() && trae_path.is_none() {
        import_both_defaults(store, paths)
    } else {
        import_both_files(
            store,
            wb_path.map(|p| p.as_path()),
            trae_path.map(|p| p.as_path()),
            paths,
        )
    };

    match serde_json::to_string_pretty(&both) {
        Ok(s) => {
            println!("{s}");
            if both.any_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("serialize report: {e}");
            ExitCode::from(1)
        }
    }
}
