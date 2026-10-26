//! Persistence: atomic JSON files, corruption recovery, the settings/profile/layout
//! store, configuration backups and the session archive.

use crate::analysis::Verdict;
use crate::defaults;
use crate::layouts::{presets, Layout};
use crate::model::{AlertRule, Profile, Settings};
use crate::session::{SessionData, SessionMeta};
use crate::util::{now_ms, stamp};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const BACKUP_FORMAT: &str = "fpsframer-backup";

// ───────────────────────────── paths ─────────────────────────────

#[derive(Debug, Clone)]
pub struct Paths {
    /// Small, user-editable files (settings, layouts, profiles).
    pub config: PathBuf,
    /// Larger/transient data (sessions, logs, backups).
    pub data: PathBuf,
}

impl Paths {
    /// `FPSFRAMER_DATA_DIR` overrides both locations (used by tests and portable setups).
    pub fn from_env() -> Paths {
        if let Some(dir) = std::env::var_os("FPSFRAMER_DATA_DIR").filter(|s| !s.is_empty()) {
            let d = PathBuf::from(dir);
            return Paths { config: d.join("config"), data: d.join("data") };
        }
        let config = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("FPSFramer");
        let data = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("FPSFramer");
        Paths { config, data }
    }
    pub fn in_dir(dir: &Path) -> Paths {
        Paths { config: dir.join("config"), data: dir.join("data") }
    }
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.json")
    }
    pub fn rules_file(&self) -> PathBuf {
        self.config.join("alerts.json")
    }
    pub fn layouts_dir(&self) -> PathBuf {
        self.config.join("layouts")
    }
    pub fn profiles_dir(&self) -> PathBuf {
        self.config.join("profiles")
    }
    pub fn sessions_dir(&self) -> PathBuf {
        self.data.join("sessions")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.data.join("backups")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.data.join("logs")
    }
}

/// Ids become file names, so only allow a safe alphabet.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') && !id.starts_with('.')
}

// ───────────────────────────── file helpers ─────────────────────────────

/// Write via a temp file + rename so a crash or power loss never leaves a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8], keep_backup: bool) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if keep_backup && path.exists() {
        let _ = fs::copy(path, backup_path(path));
    }
    fs::rename(&tmp, path)
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

pub fn write_json<T: Serialize>(path: &Path, value: &T, keep_backup: bool) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    write_atomic(path, &bytes, keep_backup)
}

pub enum Loaded<T> {
    Missing,
    Ok(T),
    /// The main file was damaged; an older copy was used. The damaged file was kept.
    Recovered(T, String),
    /// Nothing usable. The damaged file was kept.
    Failed(String),
}

pub fn load_json<T: DeserializeOwned>(path: &Path) -> Loaded<T> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Loaded::Missing,
        Err(e) => return Loaded::Failed(format!("Could not read {}: {e}", path.display())),
    };
    match serde_json::from_slice::<T>(&bytes) {
        Ok(v) => Loaded::Ok(v),
        Err(e) => {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let quarantine = path.with_file_name(format!("{name}.corrupt-{}", stamp(now_ms())));
            let _ = fs::rename(path, &quarantine);
            let note = format!("{name} was damaged ({e}). The damaged copy was kept as {}.", quarantine.display());
            if let Ok(b) = fs::read(backup_path(path)) {
                if let Ok(v) = serde_json::from_slice::<T>(&b) {
                    return Loaded::Recovered(v, format!("{note} Restored your previous version."));
                }
            }
            Loaded::Failed(note)
        }
    }
}

pub fn write_json_gz<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let json = serde_json::to_vec(value).map_err(io::Error::other)?;
    let mut enc = GzEncoder::new(Vec::new(), Compression::new(6));
    enc.write_all(&json)?;
    write_atomic(path, &enc.finish()?, false)
}

pub fn read_json_gz<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let f = File::open(path)?;
    let mut s = Vec::new();
    GzDecoder::new(f).read_to_end(&mut s)?;
    serde_json::from_slice(&s).map_err(io::Error::other)
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    v.sort();
    v
}

// ───────────────────────────── store ─────────────────────────────

pub struct Store {
    pub paths: Paths,
    pub settings: Settings,
    /// User layouts only; presets are merged in by [`Store::all_layouts`].
    pub layouts: Vec<Layout>,
    pub profiles: Vec<Profile>,
    pub rules: Vec<AlertRule>,
}

pub struct Opened {
    pub store: Store,
    /// Plain-language messages for problems that were repaired while loading.
    pub notices: Vec<String>,
    pub first_run: bool,
}

impl Store {
    pub fn open(paths: Paths) -> Opened {
        let mut notices = Vec::new();
        let (mut settings, first_run) = match load_json::<Settings>(&paths.settings_file()) {
            Loaded::Ok(s) => (s, false),
            Loaded::Missing => (defaults::default_settings(), true),
            Loaded::Recovered(s, n) => {
                notices.push(n);
                (s, false)
            }
            Loaded::Failed(n) => {
                notices.push(format!("{n} Default settings are being used."));
                (defaults::default_settings(), false)
            }
        };
        settings.sanitize();

        let mut layouts = Vec::new();
        for f in json_files(&paths.layouts_dir()) {
            match fs::read(&f).ok().and_then(|b| serde_json::from_slice::<Layout>(&b).ok()) {
                Some(mut l) => {
                    l.builtin = false;
                    if !valid_id(&l.id) {
                        l.id = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    }
                    if !valid_id(&l.id) || l.sanitize().is_err() {
                        notices.push(format!("The layout file {} is invalid and was skipped.", f.display()));
                        continue;
                    }
                    layouts.push(l);
                }
                None => notices.push(format!("The layout file {} could not be read and was skipped.", f.display())),
            }
        }

        let mut profiles: Vec<Profile> = Vec::new();
        for f in json_files(&paths.profiles_dir()) {
            match fs::read(&f).ok().and_then(|b| serde_json::from_slice::<Profile>(&b).ok()) {
                Some(mut p) => {
                    if !valid_id(&p.id) {
                        p.id = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    }
                    if !valid_id(&p.id) {
                        continue;
                    }
                    p.sanitize();
                    profiles.push(p);
                }
                None => notices.push(format!("The profile file {} could not be read and was skipped.", f.display())),
            }
        }
        let mut seeded = false;
        if profiles.is_empty() && first_run {
            profiles = defaults::default_profiles();
            seeded = true;
        } else if !profiles.iter().any(|p| p.id == defaults::DEFAULT_PROFILE_ID) {
            // The "All games" profile must always exist.
            profiles.insert(0, defaults::default_profiles().remove(0));
            seeded = true;
        }

        let rules = match load_json::<Vec<AlertRule>>(&paths.rules_file()) {
            Loaded::Ok(r) => r,
            Loaded::Missing => defaults::default_rules(),
            Loaded::Recovered(r, n) => {
                notices.push(n);
                r
            }
            Loaded::Failed(n) => {
                notices.push(format!("{n} Default alerts are being used."));
                defaults::default_rules()
            }
        };
        let mut rules = rules;
        rules.iter_mut().for_each(|r| r.sanitize());

        let store = Store { paths, settings, layouts, profiles, rules };
        if first_run || seeded {
            let _ = store.save_settings();
            let _ = store.save_rules();
            for p in &store.profiles {
                let _ = store.save_profile(&p.id);
            }
        }
        Opened { store, notices, first_run }
    }

    pub fn save_settings(&self) -> io::Result<()> {
        write_json(&self.paths.settings_file(), &self.settings, true)
    }
    pub fn save_rules(&self) -> io::Result<()> {
        write_json(&self.paths.rules_file(), &self.rules, true)
    }
    pub fn save_layout(&self, id: &str) -> io::Result<()> {
        let l = self.layouts.iter().find(|l| l.id == id).ok_or_else(|| io::Error::other("unknown layout"))?;
        write_json(&self.paths.layouts_dir().join(format!("{id}.json")), l, false)
    }
    pub fn remove_layout_file(&self, id: &str) {
        if valid_id(id) {
            let _ = fs::remove_file(self.paths.layouts_dir().join(format!("{id}.json")));
        }
    }
    pub fn save_profile(&self, id: &str) -> io::Result<()> {
        let p = self.profiles.iter().find(|p| p.id == id).ok_or_else(|| io::Error::other("unknown profile"))?;
        write_json(&self.paths.profiles_dir().join(format!("{id}.json")), p, false)
    }
    pub fn remove_profile_file(&self, id: &str) {
        if valid_id(id) {
            let _ = fs::remove_file(self.paths.profiles_dir().join(format!("{id}.json")));
        }
    }

    /// Presets first, then the user's own layouts.
    pub fn all_layouts(&self) -> Vec<Layout> {
        let mut v = presets();
        v.extend(self.layouts.iter().cloned());
        v
    }
    pub fn layout(&self, id: &str) -> Option<Layout> {
        self.layouts.iter().find(|l| l.id == id).cloned().or_else(|| crate::layouts::preset_by_id(id))
    }

    // ── backups ────────────────────────────────────────────────

    pub fn bundle(&self) -> BackupBundle {
        BackupBundle {
            format: BACKUP_FORMAT.into(),
            version: 1,
            created_at: now_ms(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            settings: self.settings.clone(),
            layouts: self.layouts.clone(),
            profiles: self.profiles.clone(),
            alert_rules: self.rules.clone(),
        }
    }

    /// Replace everything with the bundle's content and persist it.
    pub fn apply_bundle(&mut self, mut b: BackupBundle) -> Result<(), String> {
        b.validate()?;
        for l in &self.layouts {
            self.remove_layout_file(&l.id);
        }
        for p in &self.profiles {
            self.remove_profile_file(&p.id);
        }
        self.settings = b.settings;
        self.layouts = b.layouts;
        self.profiles = b.profiles;
        self.rules = b.alert_rules;
        self.save_settings().map_err(|e| e.to_string())?;
        self.save_rules().map_err(|e| e.to_string())?;
        for l in self.layouts.clone() {
            self.save_layout(&l.id).map_err(|e| e.to_string())?;
        }
        for p in self.profiles.clone() {
            self.save_profile(&p.id).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Write a timestamped backup file; keeps the newest `keep` automatic ones.
    pub fn write_backup(&self, label: &str, keep: usize) -> io::Result<PathBuf> {
        let dir = self.paths.backups_dir();
        let path = dir.join(format!("fpsframer-{label}-{}.json", stamp(now_ms())));
        write_json(&path, &self.bundle(), false)?;
        let prefix = format!("fpsframer-{label}-");
        let mut files: Vec<PathBuf> = json_files(&dir)
            .into_iter()
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&prefix)))
            .collect();
        files.sort();
        while files.len() > keep {
            let _ = fs::remove_file(files.remove(0));
        }
        Ok(path)
    }

    pub fn list_backups(&self) -> Vec<BackupInfo> {
        let mut v: Vec<BackupInfo> = json_files(&self.paths.backups_dir())
            .into_iter()
            .filter_map(|p| {
                let meta = fs::metadata(&p).ok()?;
                Some(BackupInfo {
                    name: p.file_name()?.to_string_lossy().to_string(),
                    size: meta.len(),
                    modified: meta.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64),
                })
            })
            .collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.modified));
        v
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub size: u64,
    pub modified: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct BackupBundle {
    pub format: String,
    pub version: u32,
    pub created_at: u64,
    pub app_version: String,
    pub settings: Settings,
    pub layouts: Vec<Layout>,
    pub profiles: Vec<Profile>,
    pub alert_rules: Vec<AlertRule>,
}


impl BackupBundle {
    /// Checks the file really is a backup and repairs/validates its content.
    pub fn validate(&mut self) -> Result<(), String> {
        if self.format != BACKUP_FORMAT {
            return Err("This file is not an FPSFramer backup.".into());
        }
        if self.version > 1 {
            return Err("This backup was made by a newer version of FPSFramer. Please update the app first.".into());
        }
        self.settings.sanitize();
        let mut ok_layouts = Vec::new();
        for mut l in std::mem::take(&mut self.layouts) {
            l.builtin = false;
            if valid_id(&l.id) && l.sanitize().is_ok() {
                ok_layouts.push(l);
            }
        }
        self.layouts = ok_layouts;
        self.profiles.retain(|p| valid_id(&p.id));
        self.profiles.iter_mut().for_each(|p| p.sanitize());
        if !self.profiles.iter().any(|p| p.id == defaults::DEFAULT_PROFILE_ID) {
            self.profiles.insert(0, defaults::default_profiles().remove(0));
        }
        self.alert_rules.iter_mut().for_each(|r| r.sanitize());
        Ok(())
    }
}

// ───────────────────────────── session archive ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListItem {
    pub meta: SessionMeta,
    pub frames: u64,
    pub fps_avg: Option<f64>,
    pub low1: Option<f64>,
    pub low01: Option<f64>,
    pub spike_count: u32,
    pub drop_count: u32,
    pub verdict: Option<Verdict>,
    pub warnings: u32,
}

impl SessionListItem {
    pub fn from_data(d: &SessionData) -> Self {
        let s = &d.summary;
        Self {
            meta: d.meta.clone(),
            frames: s.frames,
            fps_avg: s.fps_avg,
            low1: s.low1,
            low01: s.low01,
            spike_count: s.spike_count,
            drop_count: s.drop_count,
            verdict: s.bottleneck.as_ref().map(|b| b.verdict),
            warnings: s.findings.iter().filter(|f| f.level == crate::analysis::FindingLevel::Warning).count() as u32,
        }
    }
}

pub struct SessionStore {
    dir: PathBuf,
}

impl SessionStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json.gz"))
    }
    fn trash(&self) -> PathBuf {
        self.dir.join("trash")
    }
    fn index_file(&self) -> PathBuf {
        self.dir.join("index.json")
    }

    pub fn save(&self, data: &SessionData) -> io::Result<()> {
        if !valid_id(&data.meta.id) {
            return Err(io::Error::other("invalid session id"));
        }
        write_json_gz(&self.file(&data.meta.id), data)?;
        let mut idx = self.list();
        idx.retain(|i| i.meta.id != data.meta.id);
        idx.push(SessionListItem::from_data(data));
        self.write_index(idx)
    }

    pub fn load(&self, id: &str) -> io::Result<SessionData> {
        if !valid_id(id) {
            return Err(io::Error::other("invalid session id"));
        }
        read_json_gz(&self.file(id))
    }

    fn write_index(&self, mut idx: Vec<SessionListItem>) -> io::Result<()> {
        idx.sort_by_key(|i| std::cmp::Reverse(i.meta.started_at));
        write_json(&self.index_file(), &idx, false)
    }

    /// Newest first. Rebuilds the index from the session files if it is missing or stale.
    pub fn list(&self) -> Vec<SessionListItem> {
        let files: Vec<String> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json.gz")).map(str::to_string))
            .collect();
        let cached: Vec<SessionListItem> = match load_json::<Vec<SessionListItem>>(&self.index_file()) {
            Loaded::Ok(v) => v,
            _ => Vec::new(),
        };
        let in_sync = cached.len() == files.len() && cached.iter().all(|c| files.contains(&c.meta.id));
        if in_sync {
            return cached;
        }
        let mut rebuilt: Vec<SessionListItem> = files
            .iter()
            .filter_map(|id| cached.iter().find(|c| &c.meta.id == id).cloned().or_else(|| self.load(id).ok().map(|d| SessionListItem::from_data(&d))))
            .collect();
        rebuilt.sort_by_key(|i| std::cmp::Reverse(i.meta.started_at));
        let _ = write_json(&self.index_file(), &rebuilt, false);
        rebuilt
    }

    /// Move to a trash folder so a delete can be undone.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        if !valid_id(id) {
            return Err(io::Error::other("invalid session id"));
        }
        fs::create_dir_all(self.trash())?;
        fs::rename(self.file(id), self.trash().join(format!("{id}.json.gz")))?;
        let mut idx = self.list();
        idx.retain(|i| i.meta.id != id);
        self.write_index(idx)
    }

    pub fn restore(&self, id: &str) -> io::Result<()> {
        if !valid_id(id) {
            return Err(io::Error::other("invalid session id"));
        }
        fs::rename(self.trash().join(format!("{id}.json.gz")), self.file(id))?;
        let _ = fs::remove_file(self.index_file()); // force rebuild
        Ok(())
    }

    pub fn update_meta(&self, id: &str, f: impl FnOnce(&mut SessionMeta)) -> io::Result<SessionMeta> {
        let mut d = self.load(id)?;
        f(&mut d.meta);
        self.save(&d)?;
        Ok(d.meta)
    }

    /// Permanently remove trashed sessions older than `days`, and the oldest sessions beyond `max` (0 = unlimited).
    pub fn housekeeping(&self, days: u64, max: u32) {
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86_400);
        for e in fs::read_dir(self.trash()).into_iter().flatten().flatten() {
            if e.metadata().and_then(|m| m.modified()).is_ok_and(|m| m < cutoff) {
                let _ = fs::remove_file(e.path());
            }
        }
        if max > 0 {
            let idx = self.list();
            // Starred sessions are never auto-removed.
            let removable: Vec<&SessionListItem> = idx.iter().filter(|i| !i.meta.starred).collect();
            if idx.len() > max as usize {
                let excess = idx.len() - max as usize;
                for item in removable.iter().rev().take(excess) {
                    let _ = fs::remove_file(self.file(&item.meta.id));
                }
                let _ = fs::remove_file(self.index_file());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Profile, Theme};
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fpsframer-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn first_run_seeds_defaults_and_second_run_loads_them() {
        let dir = tmp();
        let o = Store::open(Paths::in_dir(&dir));
        assert!(o.first_run);
        assert!(o.store.profiles.iter().any(|p| p.id == "default"));
        assert!(!o.store.rules.is_empty());
        let mut s = o.store;
        s.settings.theme = Theme::Light;
        s.save_settings().unwrap();
        let again = Store::open(Paths::in_dir(&dir));
        assert!(!again.first_run);
        assert_eq!(again.store.settings.theme, Theme::Light);
        assert_eq!(again.store.profiles.len(), 3);
    }

    #[test]
    fn corrupt_settings_are_quarantined_and_previous_version_restored() {
        let dir = tmp();
        let paths = Paths::in_dir(&dir);
        let mut s = Store::open(paths.clone()).store;
        s.settings.theme = Theme::Light;
        s.save_settings().unwrap();
        s.settings.theme = Theme::Dark;
        s.save_settings().unwrap(); // now settings.json = dark, .bak = light
        fs::write(paths.settings_file(), b"{ this is not json").unwrap();
        let o = Store::open(paths.clone());
        assert!(!o.notices.is_empty(), "the user must be told");
        assert!(o.notices[0].contains("damaged"));
        assert_eq!(o.store.settings.theme, Theme::Light, "restored from the .bak copy");
        let kept = fs::read_dir(&paths.config).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains(".corrupt-"));
        assert!(kept, "damaged file is preserved for inspection");
    }

    #[test]
    fn user_layouts_roundtrip_and_presets_are_merged() {
        let dir = tmp();
        let mut s = Store::open(Paths::in_dir(&dir)).store;
        let mut l = crate::layouts::preset_by_id("preset.gaming").unwrap().duplicate(Some("Mine".into()));
        l.sanitize().unwrap();
        let id = l.id.clone();
        s.layouts.push(l);
        s.save_layout(&id).unwrap();
        let s2 = Store::open(Paths::in_dir(&dir)).store;
        assert!(s2.layout(&id).is_some());
        assert!(s2.all_layouts().iter().any(|l| l.id == "preset.minimal"));
        assert!(s2.all_layouts().iter().any(|l| l.name == "Mine" && !l.builtin));
    }

    #[test]
    fn deleting_all_profiles_still_leaves_the_default() {
        let dir = tmp();
        let s = Store::open(Paths::in_dir(&dir)).store;
        for p in &s.profiles {
            s.remove_profile_file(&p.id);
        }
        let s2 = Store::open(Paths::in_dir(&dir)).store;
        assert!(s2.profiles.iter().any(|p| p.id == "default"));
    }

    #[test]
    fn backup_roundtrip_and_validation() {
        let dir = tmp();
        let mut s = Store::open(Paths::in_dir(&dir)).store;
        s.profiles.push(Profile { id: "p_extra".into(), name: "Extra".into(), ..Default::default() });
        let b = s.bundle();
        let json = serde_json::to_string(&b).unwrap();
        let mut restored: BackupBundle = serde_json::from_str(&json).unwrap();
        restored.validate().unwrap();
        s.profiles.clear();
        s.apply_bundle(restored).unwrap();
        assert!(s.profiles.iter().any(|p| p.id == "p_extra"));

        let mut wrong: BackupBundle = serde_json::from_str(r#"{"format":"something-else"}"#).unwrap();
        assert!(wrong.validate().is_err());
        let mut newer: BackupBundle = serde_json::from_str(r#"{"format":"fpsframer-backup","version":99}"#).unwrap();
        assert!(newer.validate().unwrap_err().contains("newer"));
    }

    #[test]
    fn backup_rotation_keeps_only_newest() {
        let dir = tmp();
        let s = Store::open(Paths::in_dir(&dir)).store;
        let d = s.paths.backups_dir();
        fs::create_dir_all(&d).unwrap();
        for i in 0..5 {
            fs::write(d.join(format!("fpsframer-auto-2020010{i}-000000.json")), b"{}").unwrap();
        }
        s.write_backup("auto", 3).unwrap();
        assert_eq!(s.list_backups().len(), 3);
    }

    #[test]
    fn session_store_save_list_delete_restore() {
        let dir = tmp();
        let st = SessionStore::new(dir.join("sessions"));
        let mut d = crate::session::SessionRecorder::new(
            SessionMeta { id: "s_one".into(), started_at: 100, ..Default::default() },
            0,
            crate::session::RecorderConfig { record_all_sensors: false, target_fps: None, lows_method: Default::default() },
        )
        .finish(1_000_000_000, 200);
        st.save(&d).unwrap();
        d.meta.id = "s_two".into();
        d.meta.started_at = 300;
        st.save(&d).unwrap();
        let l = st.list();
        assert_eq!(l.iter().map(|i| i.meta.id.as_str()).collect::<Vec<_>>(), vec!["s_two", "s_one"], "newest first");
        st.delete("s_two").unwrap();
        assert_eq!(st.list().len(), 1);
        assert!(st.load("s_two").is_err());
        st.restore("s_two").unwrap();
        assert_eq!(st.list().len(), 2);
        st.update_meta("s_one", |m| m.starred = true).unwrap();
        assert!(st.load("s_one").unwrap().meta.starred);
        assert!(st.load("../etc/passwd").is_err(), "ids are validated against path traversal");
    }

    #[test]
    fn session_index_rebuilds_when_missing() {
        let dir = tmp();
        let st = SessionStore::new(dir.join("sessions"));
        let d = crate::session::SessionRecorder::new(
            SessionMeta { id: "s_x".into(), ..Default::default() },
            0,
            crate::session::RecorderConfig { record_all_sensors: false, target_fps: None, lows_method: Default::default() },
        )
        .finish(1_000_000_000, 2);
        st.save(&d).unwrap();
        fs::remove_file(dir.join("sessions").join("index.json")).unwrap();
        assert_eq!(st.list().len(), 1);
    }

    #[test]
    fn id_validation() {
        assert!(valid_id("lay_1a2b3c4d"));
        assert!(valid_id("preset.gaming"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(""));
        assert!(!valid_id(".hidden"));
        assert!(!valid_id("a/b"));
    }
}
