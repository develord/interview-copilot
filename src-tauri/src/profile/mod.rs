//! The candidate's verified profile (PRD §5, §8): the only source of personal facts in
//! answers. It is extracted once from a CV PDF (`extract`), reviewed and corrected by the user,
//! and saved locally as `profile.json` in the app data directory. Nothing inferred is stored
//! as verified: the user validates every field before it is used.

pub mod commands;
pub mod extract;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Per field; a CV section rarely needs more.
pub const MAX_FIELD_CHARS: usize = 8_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub name: String,
    /// Current title or one-line positioning.
    pub headline: String,
    pub summary: String,
    pub skills: String,
    /// One block per position: role, company, period, then facts.
    pub experience: String,
    pub projects: String,
    pub education: String,
    pub languages: String,
    /// The role or job description the interview is for (optional, user-provided).
    pub target_role: String,
    /// Unix ms when the user validated the profile; `None` for an unreviewed draft.
    pub verified_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileErrorCode {
    /// A field is too long, or the file is not a PDF.
    Invalid,
    /// Reading or writing `profile.json` failed.
    Storage,
    /// No Claude API key is stored.
    MissingKey,
    /// The Claude call failed (see `detail`).
    Extraction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProfileError {
    pub code: ProfileErrorCode,
    pub detail: String,
}

impl ProfileError {
    pub fn new(code: ProfileErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl Profile {
    fn fields(&self) -> [(&'static str, &str); 9] {
        [
            ("name", &self.name),
            ("headline", &self.headline),
            ("summary", &self.summary),
            ("skills", &self.skills),
            ("experience", &self.experience),
            ("projects", &self.projects),
            ("education", &self.education),
            ("languages", &self.languages),
            ("targetRole", &self.target_role),
        ]
    }

    /// Trims every field and rejects oversized ones.
    pub fn normalized(mut self) -> Result<Self, ProfileError> {
        for field in [
            &mut self.name,
            &mut self.headline,
            &mut self.summary,
            &mut self.skills,
            &mut self.experience,
            &mut self.projects,
            &mut self.education,
            &mut self.languages,
            &mut self.target_role,
        ] {
            *field = field.trim().replace("\r\n", "\n");
        }
        if let Some((name, _)) = self
            .fields()
            .into_iter()
            .find(|(_, v)| v.chars().count() > MAX_FIELD_CHARS)
        {
            return Err(ProfileError::new(
                ProfileErrorCode::Invalid,
                format!("{name} is longer than {MAX_FIELD_CHARS} characters"),
            ));
        }
        Ok(self)
    }

    pub fn is_empty(&self) -> bool {
        self.fields().iter().all(|(_, v)| v.trim().is_empty())
    }

    /// The profile as the answer prompt sees it. Empty sections are left out; speech-
    /// recognition-style angle brackets cannot close the tag.
    pub fn render(&self) -> String {
        const LABELS: [&str; 9] = [
            "Name",
            "Headline",
            "Summary",
            "Skills",
            "Experience",
            "Projects",
            "Education",
            "Languages",
            "Target role",
        ];
        let mut out = String::new();
        for ((_, value), label) in self.fields().into_iter().zip(LABELS) {
            if !value.trim().is_empty() {
                out.push_str(&format!(
                    "## {label}\n{}\n\n",
                    value.replace(['<', '>'], " ")
                ));
            }
        }
        out.trim_end().to_string()
    }
}

/// The verified profile on disk, cached in memory.
pub struct ProfileStore {
    path: Option<PathBuf>,
    current: Mutex<Option<Profile>>,
}

impl ProfileStore {
    /// `path` is `None` when the app data directory is unavailable: the profile then lives in
    /// memory only.
    pub fn open(path: Option<PathBuf>) -> Self {
        let current = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str::<Profile>(&text).ok())
            .filter(|p| p.verified_at.is_some());
        Self {
            path,
            current: Mutex::new(current),
        }
    }

    pub fn get(&self) -> Option<Profile> {
        self.current.lock().ok().and_then(|p| p.clone())
    }

    /// Saves a reviewed profile and marks it verified.
    pub fn save(&self, profile: Profile, now_ms: u64) -> Result<Profile, ProfileError> {
        let mut profile = profile.normalized()?;
        profile.verified_at = Some(now_ms);
        if let Some(path) = &self.path {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| ProfileError::new(ProfileErrorCode::Storage, e.to_string()))?;
            }
            let json = serde_json::to_string_pretty(&profile)
                .map_err(|e| ProfileError::new(ProfileErrorCode::Storage, e.to_string()))?;
            std::fs::write(path, json)
                .map_err(|e| ProfileError::new(ProfileErrorCode::Storage, e.to_string()))?;
        }
        if let Ok(mut current) = self.current.lock() {
            *current = Some(profile.clone());
        }
        Ok(profile)
    }

    pub fn clear(&self) -> Result<(), ProfileError> {
        if let Some(path) = &self.path {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(ProfileError::new(ProfileErrorCode::Storage, e.to_string())),
            }
        }
        if let Ok(mut current) = self.current.lock() {
            *current = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        Profile {
            name: "  Alex Martin ".into(),
            headline: "Développeur front-end".into(),
            skills: "Vue.js\r\nTypeScript".into(),
            experience: "Développeur — Acme — 2021-2024\n- Migration Vue 2 → Vue 3".into(),
            ..Profile::default()
        }
    }

    #[test]
    fn normalizing_trims_and_rejects_oversized_fields() {
        let p = profile().normalized().unwrap();
        assert_eq!(p.name, "Alex Martin");
        assert_eq!(p.skills, "Vue.js\nTypeScript");
        let huge = Profile {
            projects: "x".repeat(MAX_FIELD_CHARS + 1),
            ..Profile::default()
        };
        assert_eq!(
            huge.normalized().unwrap_err().code,
            ProfileErrorCode::Invalid
        );
    }

    #[test]
    fn rendering_keeps_filled_sections_only_and_neutralizes_tags() {
        let mut p = profile().normalized().unwrap();
        p.summary = "Aime les <balises>".into();
        let text = p.render();
        assert!(text.starts_with("## Name\nAlex Martin"));
        assert!(text.contains("## Experience\nDéveloppeur — Acme — 2021-2024"));
        assert!(!text.contains("## Projects"));
        assert!(text.contains("Aime les  balises "));
        assert!(Profile::default().is_empty());
    }

    #[test]
    fn the_store_round_trips_through_disk_and_ignores_unverified_files() {
        let dir = std::env::temp_dir().join(format!("ic-profile-{}", std::process::id()));
        let path = dir.join("profile.json");
        let _ = std::fs::remove_dir_all(&dir);

        let store = ProfileStore::open(Some(path.clone()));
        assert_eq!(store.get(), None);
        let saved = store.save(profile(), 42).unwrap();
        assert_eq!(saved.verified_at, Some(42));
        assert_eq!(ProfileStore::open(Some(path.clone())).get(), Some(saved));

        // A draft written by hand (never validated) is not trusted.
        std::fs::write(&path, r#"{"name":"Draft"}"#).unwrap();
        assert_eq!(ProfileStore::open(Some(path.clone())).get(), None);

        store.clear().unwrap();
        assert!(!path.exists());
        store.clear().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
