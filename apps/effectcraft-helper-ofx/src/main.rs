//! EffectCraft's OpenFX helper: the in-process OFX host of `effectcraft-ofx`, moved into its own
//! program so a plug-in that crashes or hangs takes down the helper, not EffectCraft
//! ([`docs/external-helpers.md`](../../../docs/external-helpers.md)).
//!
//! EffectCraft starts this program as `effectcraft-helper-ofx --effectcraft-helper` (the argument
//! is the helper protocol's, `effectcraft_helper_host::protocol::HELPER_ARG`) with the frame
//! exchange file named in `hello`. It scans the OpenFX folders once at start-up, offers every
//! plug-in as an ordinary effect, and renders frames on request. Plug-ins that crashed it are
//! remembered in the same `ofx_blocklist.txt` / `ofx_render_crashes.txt` the in-process host used,
//! in the state folder EffectCraft names in `EFFECTCRAFT_HELPER_STATE_DIR`.
//!
//! Search folders are the ones [`effectcraft_ofx::default_search_paths`] knows (the system's OFX
//! folders, the per-user ones, `OFX` in the state folder and next to this program, and
//! `OFX_PLUGIN_PATH`), plus `OFX` next to EffectCraft itself (the app folder's `OFX` is one level
//! above `helpers/`, where this program is installed).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use effectcraft_effects::plugin::EffectPlugin;
use effectcraft_helper_host::serve::{Provider, serve};

/// The effects this helper offers, in the order the scan found them.
struct OfxProvider {
    /// The folders and binaries the start-up scan searches.
    paths: Vec<PathBuf>,
    /// Effect ids already sent to EffectCraft (a scan or a load can reach the same plug-in again).
    offered: HashSet<String>,
    /// The start-up scan has run (`effects` is asked once, but a second call must not rescan).
    described: bool,
}

impl OfxProvider {
    fn new() -> OfxProvider {
        OfxProvider { paths: search_paths(), offered: HashSet::new(), described: false }
    }

    /// The effects registered since the last call, each once.
    fn new_effects(&mut self) -> Vec<Arc<dyn EffectPlugin>> {
        let mut out = Vec::new();
        for effect in effectcraft_ofx::registered_effects() {
            if self.offered.insert(effect.manifest().id.clone()) {
                out.push(effect);
            }
        }
        out
    }
}

impl Provider for OfxProvider {
    fn name(&self) -> String {
        "OpenFX".into()
    }

    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn loads(&self) -> Vec<String> {
        vec!["ofx".into(), "bundle".into()]
    }

    fn effects(&mut self) -> (Vec<Arc<dyn EffectPlugin>>, Vec<String>) {
        if self.described {
            return (vec![], vec![]);
        }
        self.described = true;
        let report = effectcraft_ofx::load_paths_with(&self.paths, true);
        // A standard folder that does not exist on this machine is normal, not a problem the user
        // has to see; the scan report in EffectCraft lists every folder it searched. `notes` are
        // informational (a duplicate copy skipped, a context fallback), not failures.
        let errors = report.errors.into_iter().filter(|e| !e.ends_with(": no such file or folder")).collect();
        (self.new_effects(), errors)
    }

    fn load(&mut self, path: &str) -> Result<(Vec<Arc<dyn EffectPlugin>>, Vec<String>), String> {
        let report = effectcraft_ofx::load_path_with(Path::new(path), false);
        let effects = self.new_effects();
        let errors = report.errors;
        if effects.is_empty() && !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok((effects, errors))
    }
}

/// Where the plug-ins are looked for: the OFX host's own list, plus `OFX` in the app folder
/// (the parent of `helpers/`, where EffectCraft installs this program).
fn search_paths() -> Vec<PathBuf> {
    let mut v = effectcraft_ofx::default_search_paths();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        if let Some(app) = dir.parent() {
            v.push(app.join("OFX"));
        }
        v.push(dir.join("OFX"));
    }
    v
}

/// EffectCraft's settings folder: `EFFECTCRAFT_HELPER_STATE_DIR` (what EffectCraft passes), else
/// `EFFECTCRAFT_CONFIG_DIR`, else the platform's default. The OFX host keeps its crash lists, its
/// scan cache and `ofx_settings.json` there, and searches `OFX` in it.
fn state_dir() -> Option<PathBuf> {
    for key in ["EFFECTCRAFT_HELPER_STATE_DIR", "EFFECTCRAFT_CONFIG_DIR"] {
        if let Some(dir) = std::env::var_os(key).filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(dir));
        }
    }
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        std::env::var_os("APPDATA").filter(|a| !a.is_empty()).map(|a| PathBuf::from(a).join("EffectCraft"))
    } else if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/EffectCraft"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.to_string_lossy().starts_with('/'))
            .or_else(|| home.map(|h| h.join(".config")))
            .map(|c| c.join("effectcraft"))
    }
}

fn main() {
    if let Some(dir) = state_dir() {
        effectcraft_ofx::set_state_dir(dir);
    }
    let mut provider = OfxProvider::new();
    let result = serve(&mut provider);
    // Destroy the pooled plug-in instances and unload every plug-in: a plug-in that keeps helper
    // programs running (Sapphire's preset browser) would otherwise outlive this process.
    effectcraft_ofx::shutdown();
    if let Err(e) = result {
        eprintln!("effectcraft-helper-ofx: {e}");
        std::process::exit(1);
    }
}
