pub mod hypergryph;
pub mod manager;

pub use hypergryph::{fetch_game_manifest, fetch_patch_manifest, GameManifest};
pub use manager::{
    check_game_installed, fetch_latest_version, read_local_version,
    require_game_exe, validate_install_path,
};
