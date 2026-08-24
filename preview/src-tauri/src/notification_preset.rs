use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPreset {
    pub width: u16,
    pub height: u16,
    pub duration_ms: u64,
}

pub fn resolve(name: &str) -> NotificationPreset {
    let (width, height) = match name {
        "default" | "original" => (420, 110),
        "ps4" => (400, 200),
        "ps5" => (400, 150),
        "ps5_enhanced" => (450, 150),
        "xbox_one" => (600, 160),
        "xbox_360" => (600, 150),
        "raposo" | "smooth_pop" => (400, 150),
        "xqjan" => (450, 150),
        _ => (382, 106),
    };
    let duration_ms = match name {
        "default" | "original" | "raposo" => 6_000,
        "ps4" | "xbox_360" => 5_000,
        "smooth_pop" => 8_000,
        "xbox_one" | "xqjan" => 10_000,
        _ => 4_000,
    };
    NotificationPreset {
        width,
        height,
        duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn resolves_original_and_compact_steam_presets() {
        let original = resolve("original");
        assert_eq!(
            (original.width, original.height, original.duration_ms),
            (420, 110, 6_000)
        );
        let steam = resolve("steam");
        assert_eq!(
            (steam.width, steam.height, steam.duration_ms),
            (382, 106, 4_000)
        );
    }
}
