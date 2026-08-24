use serde::Serialize;
use std::{
    env, process, thread,
    time::{Duration, Instant},
};
use steamworks::{AppId, Client};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SteamSnapshot {
    app_id: u32,
    achievements: Vec<SteamAchievement>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SteamAchievement {
    api_name: String,
    display_name: String,
    description: String,
    hidden: bool,
    achieved: bool,
    unlock_time: u32,
}

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("{}", message.as_ref());
    process::exit(2);
}

fn main() {
    let app_id = env::args()
        .nth(1)
        .unwrap_or_else(|| fail("usage: achievement-watcher-steam-helper <app-id>"))
        .parse::<u32>()
        .unwrap_or_else(|_| fail("app ID must be an unsigned integer"));
    let client = Client::init_app(AppId(app_id))
        .unwrap_or_else(|error| fail(format!("could not connect to Steam for {app_id}: {error}")));
    let stats = client.user_stats();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        client.run_callbacks();
        if stats.get_num_achievements().is_ok() {
            break;
        }
        if Instant::now() >= deadline {
            fail(format!("Steam did not load achievement data for {app_id}"));
        }
        thread::sleep(Duration::from_millis(50));
    }
    let names = stats.get_achievement_names().unwrap_or_default();

    let achievements = names
        .into_iter()
        .map(|api_name| {
            let achievement = stats.achievement(&api_name);
            let (achieved, unlock_time) = achievement
                .get_achievement_and_unlock_time()
                .unwrap_or((false, 0));
            SteamAchievement {
                hidden: achievement
                    .get_achievement_display_attribute("hidden")
                    .is_ok_and(|value| value == "1"),
                display_name: achievement
                    .get_achievement_display_attribute("name")
                    .unwrap_or(&api_name)
                    .to_owned(),
                description: achievement
                    .get_achievement_display_attribute("desc")
                    .unwrap_or_default()
                    .to_owned(),
                api_name,
                achieved,
                unlock_time,
            }
        })
        .collect();
    let snapshot = SteamSnapshot {
        app_id,
        achievements,
    };
    println!(
        "{}",
        serde_json::to_string(&snapshot).expect("snapshot serialization failed")
    );
}
