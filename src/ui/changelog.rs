use std::sync::OnceLock;

const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeKind {
    Added,
    Changed,
    Fixed,
    Removed,
    Other,
}

impl ChangeKind {
    fn parse(heading: &str) -> Self {
        match heading.trim().to_ascii_lowercase().as_str() {
            "added" => Self::Added,
            "changed" => Self::Changed,
            "fixed" => Self::Fixed,
            "removed" | "deprecated" => Self::Removed,
            _ => Self::Other,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Added => "New",
            Self::Changed => "Improved",
            Self::Fixed => "Bug fix",
            Self::Removed => "Removed",
            Self::Other => "Note",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ChangeEntry {
    pub(crate) kind: ChangeKind,
    pub(crate) text: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Release {
    pub(crate) version: String,
    pub(crate) date: String,
    pub(crate) entries: Vec<ChangeEntry>,
}

pub(crate) fn releases() -> &'static [Release] {
    static PARSED: OnceLock<Vec<Release>> = OnceLock::new();
    PARSED.get_or_init(|| parse(CHANGELOG))
}

pub(crate) fn pending(last_seen: Option<&str>) -> Vec<&'static Release> {
    let Some(last_seen) = last_seen.and_then(parse_version) else {
        return Vec::new();
    };
    let Some(current) = parse_version(current_version()) else {
        return Vec::new();
    };
    if !is_feature_upgrade(&last_seen, &current) {
        return Vec::new();
    }

    releases()
        .iter()
        .filter(|release| {
            parse_version(&release.version)
                .is_some_and(|version| version > last_seen && version <= current)
        })
        .collect()
}

fn is_feature_upgrade(last_seen: &semver::Version, current: &semver::Version) -> bool {
    current.major > last_seen.major
        || (current.major == last_seen.major && current.minor > last_seen.minor)
}

fn parse_version(raw: &str) -> Option<semver::Version> {
    semver::Version::parse(raw.trim()).ok()
}

pub(crate) fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub(crate) fn all() -> &'static [&'static Release] {
    static ALL: OnceLock<Vec<&'static Release>> = OnceLock::new();
    ALL.get_or_init(|| releases().iter().collect())
}

fn parse(source: &str) -> Vec<Release> {
    let mut releases: Vec<Release> = Vec::new();
    let mut kind = ChangeKind::Other;

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(heading) = trimmed.strip_prefix("## ") {
            kind = ChangeKind::Other;
            if let Some((version, date)) = parse_release_heading(heading) {
                releases.push(Release {
                    version,
                    date,
                    entries: Vec::new(),
                });
            } else {
                releases.push(Release {
                    version: String::new(),
                    date: String::new(),
                    entries: Vec::new(),
                });
            }
            continue;
        }

        if let Some(heading) = trimmed.strip_prefix("### ") {
            kind = ChangeKind::parse(heading);
            continue;
        }

        let Some(release) = releases.last_mut() else {
            continue;
        };
        if release.version.is_empty() {
            continue;
        }

        if let Some(entry) = trimmed.strip_prefix("- ") {
            release.entries.push(ChangeEntry {
                kind,
                text: entry.trim().to_string(),
            });
        } else if !trimmed.is_empty()
            && line.starts_with("  ")
            && let Some(entry) = release.entries.last_mut()
        {
            entry.text.push(' ');
            entry.text.push_str(trimmed);
        }
    }

    releases.retain(|release| !release.version.is_empty() && !release.entries.is_empty());
    releases
}

fn parse_release_heading(heading: &str) -> Option<(String, String)> {
    let heading = heading.trim();
    let (version, rest) = if let Some(rest) = heading.strip_prefix('[') {
        let (version, rest) = rest.split_once(']')?;
        (version, rest)
    } else {
        heading.split_once(' ').unwrap_or((heading, ""))
    };

    parse_version(version)?;
    let date = rest
        .trim()
        .trim_start_matches('-')
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    Some((version.trim().to_string(), date))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_running_version_has_release_notes() {
        assert!(
            releases()
                .iter()
                .any(|release| release.version == current_version()),
            "CHANGELOG.md has no `## [{}]` section — every release must ship notes",
            current_version()
        );
    }

    #[test]
    fn releases_parse_newest_first_and_carry_entries() {
        let releases = releases();
        assert!(releases.len() >= 2);
        for release in releases {
            assert!(parse_version(&release.version).is_some());
            assert!(!release.entries.is_empty());
        }
        for pair in releases.windows(2) {
            let newer = parse_version(&pair[0].version).unwrap();
            let older = parse_version(&pair[1].version).unwrap();
            assert!(newer > older, "{newer} must be listed before {older}");
        }
    }

    #[test]
    fn unreleased_section_is_ignored() {
        assert!(releases().iter().all(|release| !release.version.is_empty()));
        assert!(
            !releases()
                .iter()
                .any(|release| release.version.eq_ignore_ascii_case("unreleased"))
        );
    }

    #[test]
    fn wrapped_bullets_are_joined() {
        let parsed = parse(
            "## [1.0.0] - 2026-01-01\n\n### Added\n\n- a bullet that\n  wraps onto a second line\n",
        );
        assert_eq!(
            parsed[0].entries[0].text,
            "a bullet that wraps onto a second line"
        );
    }

    #[test]
    fn kinds_map_to_user_facing_labels() {
        let parsed = parse(
            "## [1.0.0] - 2026-01-01\n\n### Added\n\n- one\n\n### Fixed\n\n- two\n\n### Security\n\n- three\n",
        );
        let kinds: Vec<ChangeKind> = parsed[0].entries.iter().map(|entry| entry.kind).collect();
        assert_eq!(
            kinds,
            vec![ChangeKind::Added, ChangeKind::Fixed, ChangeKind::Other]
        );
    }

    #[test]
    fn only_minor_and_major_bumps_are_feature_upgrades() {
        let cases = [
            ("0.2.0", "0.3.0", true),
            ("0.2.0", "1.0.0", true),
            ("0.2.0", "0.2.1", false),
            ("0.2.0", "0.2.0", false),
            ("0.3.0", "0.2.0", false),
        ];
        for (last_seen, current, expected) in cases {
            let last_seen = parse_version(last_seen).unwrap();
            let current = parse_version(current).unwrap();
            assert_eq!(is_feature_upgrade(&last_seen, &current), expected);
        }
    }

    const KINDS: [ChangeKind; 4] = [
        ChangeKind::Added,
        ChangeKind::Changed,
        ChangeKind::Fixed,
        ChangeKind::Removed,
    ];

    fn luminance(color: iced::Color) -> f32 {
        let linear = color.into_linear();
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    }

    fn contrast_ratio(a: iced::Color, b: iced::Color) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        let (lighter, darker) = if a > b { (a, b) } else { (b, a) };
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn changelog_kind_colors_are_readable_on_every_theme() {
        for choice in crate::ui::theme::ThemeChoice::ALL {
            let theme = choice.ui_theme();
            let background = theme.extended_palette().background.base.color;
            for kind in KINDS {
                let color = crate::ui::styles::changelog_kind_color(kind, &theme);
                let ratio = contrast_ratio(color, background);
                assert!(
                    ratio >= 4.5,
                    "{choice}: {} tag has {ratio:.2}:1 contrast, needs 4.5:1",
                    kind.label()
                );
            }
        }
    }

    #[test]
    fn changelog_kind_colors_are_distinct_in_both_modes() {
        for choice in [
            crate::ui::theme::ThemeChoice::CarbonFrostNight,
            crate::ui::theme::ThemeChoice::CarbonFrostAsh,
        ] {
            let theme = choice.ui_theme();
            let colors: Vec<iced::Color> = KINDS
                .iter()
                .map(|kind| crate::ui::styles::changelog_kind_color(*kind, &theme))
                .collect();

            for (index, left) in colors.iter().enumerate() {
                for right in &colors[index + 1..] {
                    let distance = ((left.r - right.r).powi(2)
                        + (left.g - right.g).powi(2)
                        + (left.b - right.b).powi(2))
                    .sqrt();
                    assert!(
                        distance >= 0.2,
                        "{choice}: two tag colours are {distance:.2} apart, too close to tell apart"
                    );
                }
            }
        }
    }

    #[test]
    fn a_missing_baseline_announces_nothing() {
        assert!(pending(None).is_empty());
        assert!(pending(Some("not a version")).is_empty());
    }

    #[test]
    fn the_current_version_alone_announces_nothing() {
        assert!(pending(Some(current_version())).is_empty());
    }
}
