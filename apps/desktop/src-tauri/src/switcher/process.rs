//! R5：客户端进程在跑 → 硬阻断 Switching（不 kill）。

/// 按产品启发式检测进程是否在跑。
/// Windows: `tasklist`；其它: `pgrep -fi`。失败时保守返回 false（避免误阻断开发机）。
pub fn client_running_for_product(product: &str) -> bool {
    let needles: &[&str] = match product {
        "workbuddy" => &["WorkBuddy", "workbuddy", "GenieWorkBuddy"],
        "trae_cn" => &["Trae CN", "TraeCN", "Trae", "trae"],
        _ => return false,
    };
    detect_any(needles)
}

fn detect_any(needles: &[&str]) -> bool {
    #[cfg(windows)]
    {
        detect_windows(needles)
    }
    #[cfg(not(windows))]
    {
        detect_unix(needles)
    }
}

#[cfg(windows)]
fn detect_windows(needles: &[&str]) -> bool {
    let out = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output();
    let Ok(out) = out else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    needles
        .iter()
        .any(|n| text.contains(&n.to_lowercase()))
}

#[cfg(not(windows))]
fn detect_unix(needles: &[&str]) -> bool {
    for n in needles {
        // -x 精确；再 fallback -fi 模糊。任一命中即 true。
        if run_pgrep(&["-x", n]) || run_pgrep(&["-fi", n]) {
            return true;
        }
    }
    false
}

#[cfg(not(windows))]
fn run_pgrep(args: &[&str]) -> bool {
    std::process::Command::new("pgrep")
        .args(args)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}
