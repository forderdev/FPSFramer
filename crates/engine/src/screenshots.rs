//! Screenshot files: where they go, listing, thumbnails, deletion.

use base64::Engine as _;
use fpsframer_core::storage::Paths;
use fpsframer_core::util::{safe_filename, stamp};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotItem {
    /// `"<game folder>/<file>.png"` – opaque to the UI.
    pub id: String,
    pub game: String,
    pub taken_at: u64,
    pub size: u64,
}

pub struct Shots {
    root: PathBuf,
    thumbs: PathBuf,
}

/// Folder screenshots are saved in unless the user picked another.
pub fn default_root() -> PathBuf {
    dirs::picture_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")).join("FPSFramer")
}

impl Shots {
    pub fn new(folder: Option<&str>, paths: &Paths) -> Self {
        let root = folder.filter(|f| !f.trim().is_empty()).map(PathBuf::from).unwrap_or_else(default_root);
        Self { root, thumbs: paths.data.join("thumbs") }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reject anything that could escape the screenshot folder.
    fn resolve(&self, id: &str) -> Option<PathBuf> {
        let mut parts = id.split('/');
        let (game, file) = (parts.next()?, parts.next()?);
        if parts.next().is_some() || [game, file].iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(['\\', ':'])) {
            return None;
        }
        if !file.to_lowercase().ends_with(".png") {
            return None;
        }
        Some(self.root.join(game).join(file))
    }

    fn thumb_path(&self, id: &str) -> PathBuf {
        self.thumbs.join(id.replace('/', "__"))
    }

    pub fn save(&self, game: &str, png: &[u8], thumb_png: &[u8], now_ms: u64) -> std::io::Result<String> {
        let g = safe_filename(game);
        let dir = self.root.join(&g);
        fs::create_dir_all(&dir)?;
        let mut name = format!("{}.png", stamp(now_ms));
        let mut n = 1;
        while dir.join(&name).exists() {
            n += 1;
            name = format!("{}-{n}.png", stamp(now_ms));
        }
        fs::write(dir.join(&name), png)?;
        let id = format!("{g}/{name}");
        fs::create_dir_all(&self.thumbs)?;
        let _ = fs::write(self.thumb_path(&id), thumb_png);
        Ok(id)
    }

    pub fn list(&self, limit: usize) -> Vec<ShotItem> {
        let mut out = Vec::new();
        for g in fs::read_dir(&self.root).into_iter().flatten().flatten() {
            if !g.path().is_dir() {
                continue;
            }
            let game = g.file_name().to_string_lossy().to_string();
            for f in fs::read_dir(g.path()).into_iter().flatten().flatten() {
                let name = f.file_name().to_string_lossy().to_string();
                if !name.to_lowercase().ends_with(".png") {
                    continue;
                }
                let Ok(meta) = f.metadata() else { continue };
                let taken = meta.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64);
                out.push(ShotItem { id: format!("{game}/{name}"), game: game.clone(), taken_at: taken, size: meta.len() });
            }
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.taken_at));
        out.truncate(limit);
        out
    }

    /// `data:image/png;base64,…`
    pub fn data_url(&self, id: &str, thumb: bool) -> Option<String> {
        let full = self.resolve(id)?;
        let path = if thumb && self.thumb_path(id).exists() { self.thumb_path(id) } else { full };
        let bytes = fs::read(path).ok()?;
        Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    }

    pub fn path_of(&self, id: &str) -> Option<PathBuf> {
        self.resolve(id)
    }

    pub fn delete(&self, id: &str) -> bool {
        let Some(p) = self.resolve(id) else { return false };
        let _ = fs::remove_file(self.thumb_path(id));
        fs::remove_file(p).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shots() -> (Shots, PathBuf) {
        let d = std::env::temp_dir().join(format!("fpsf-shots-{}-{}", std::process::id(), fpsframer_core::util::gen_id("t")));
        let _ = fs::remove_dir_all(&d);
        (Shots::new(Some(d.join("pics").to_str().unwrap()), &Paths::in_dir(&d)), d)
    }

    #[test]
    fn save_list_read_delete() {
        let (s, _d) = shots();
        let id = s.save("Cool: Game", b"PNGDATA", b"THUMB", 1_700_000_000_000).unwrap();
        assert_eq!(id, "Cool_ Game/20231114-221320.png");
        let id2 = s.save("Cool: Game", b"PNGDATA2", b"THUMB", 1_700_000_000_000).unwrap();
        assert_ne!(id, id2, "same-second shots must not overwrite each other");
        let l = s.list(10);
        assert_eq!(l.len(), 2);
        assert!(s.data_url(&id, true).unwrap().starts_with("data:image/png;base64,"));
        assert!(s.delete(&id));
        assert_eq!(s.list(10).len(), 1);
    }

    #[test]
    fn path_traversal_is_rejected() {
        let (s, _d) = shots();
        for bad in ["../x.png", "a/../../etc/passwd", "/etc/passwd", "a/b/c.png", "a\\b.png", "a/b.txt", "..", "", "a/"] {
            assert!(s.path_of(bad).is_none(), "{bad} must be rejected");
            assert!(!s.delete(bad));
        }
    }
}
