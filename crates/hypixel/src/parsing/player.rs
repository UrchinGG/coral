use mctext::{FORMAT_SIGN, NamedColor};
use serde_json::Value;

pub fn color_code(color: &str) -> String {
    let named = NamedColor::from_name(color).unwrap_or(NamedColor::Gray);
    format!("{FORMAT_SIGN}{}", named.code())
}

pub fn calculate_network_level(exp: f64) -> f64 {
    ((2.0 * exp + 30625.0).sqrt() / 50.0) - 2.5
}

pub fn extract_rank_prefix(player: &Value) -> Option<String> {
    if let Some(prefix) = player.get("prefix").and_then(|v| v.as_str()) {
        return Some(prefix.to_string());
    }

    let rank = player.get("rank").and_then(|v| v.as_str());
    let monthly = player.get("monthlyPackageRank").and_then(|v| v.as_str());
    let new_pkg = player.get("newPackageRank").and_then(|v| v.as_str());
    let pkg = player.get("packageRank").and_then(|v| v.as_str());
    let plus_color = color_code(
        player
            .get("rankPlusColor")
            .and_then(|v| v.as_str())
            .unwrap_or("RED"),
    );
    let monthly_color = color_code(
        player
            .get("monthlyRankColor")
            .and_then(|v| v.as_str())
            .unwrap_or("GOLD"),
    );

    if let Some(r) = rank {
        match r {
            "ADMIN" => return Some("§c[ADMIN] ".to_string()),
            "GAME_MASTER" => return Some("§2[GM] ".to_string()),
            "YOUTUBER" => return Some("§c[§fYOUTUBE§c] ".to_string()),
            _ => {}
        }
    }

    if monthly == Some("SUPERSTAR") {
        return Some(format!(
            "{monthly_color}[MVP{plus_color}++§r{monthly_color}] "
        ));
    }

    match new_pkg.or(pkg) {
        Some("MVP_PLUS") => Some(format!("§b[MVP{plus_color}+§b] ")),
        Some("MVP") => Some("§b[MVP] ".to_string()),
        Some("VIP_PLUS") => Some("§a[VIP§6+§a] ".to_string()),
        Some("VIP") => Some("§a[VIP] ".to_string()),
        _ => Some("§7".to_string()),
    }
}
