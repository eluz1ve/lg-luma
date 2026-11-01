use super::*;
use serde_json::json;

/// 1 November 2026, 20:00 UTC, on a TV on UTC: its day ends at midnight.
const NOW: i64 = 1_793_563_200;
const UNTIL: i64 = 1_793_577_600;

fn entry(id: i64, name: &str, weight: i64, channel: &str) -> Value {
    json!({
        "id": id, "name": name, "weight": weight,
        "genres": ["Drama", "Thriller"], "premiered": "2024-03-01",
        "rating": {"average": 8.14},
        "image": {"medium": format!("{POSTER_ORIGIN}medium_portrait/1/{id}.jpg")},
        "webChannel": {"name": channel}
    })
}

fn installed() -> Vec<(String, String)> {
    vec![
        ("netflix".into(), "Netflix".into()),
        ("amazon.alexa.view".into(), "Alexa".into()),
    ]
}

#[test]
fn schedules_list_playable_shows_first_then_by_popularity() {
    let web = json!([
        {"_embedded": {"show": entry(1, "Quiet", 40, "Netflix")}},
        {"_embedded": {"show": entry(2, "Loud", 90, "Netflix")}},
        {"_embedded": {"show": entry(2, "Loud", 90, "Netflix")}},
        {"_embedded": {"show": entry(3, "Elsewhere", 99, "Some Channel")}},
        {"_embedded": {"show": entry(4, "Alexa only", 95, "Prime Video")}},
        {"_embedded": {"show": {"id": 5, "name": "No poster", "image": null}}}
    ]);
    let tv = json!([{"show": entry(6, "Broadcast", 60, "Disney+")}]);
    let shows = parse_schedules(&[web, tv], &installed(), UNTIL);
    let titles: Vec<&str> = shows.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Loud", "Quiet", "Elsewhere", "Alexa only", "Broadcast"]
    );
    let loud = &shows[0];
    assert_eq!(loud.app.as_deref(), Some("netflix"));
    assert_eq!(loud.service, Some("Netflix"));
    assert_eq!(loud.meta, "Drama · 2024 · ★ 8.1");
    // Alexa shares Amazon's name but does not play Prime Video.
    assert_eq!(shows[3].service, Some("Prime Video"));
    assert!(shows[3].app.is_none());
    assert!(shows[2].service.is_none());
}

#[test]
fn posters_come_only_from_tvmaze() {
    let mut value = entry(1, "Show", 1, "Netflix");
    value["image"]["medium"] = json!("https://example.com/a.jpg");
    let shows = parse_schedules(&[json!([{ "show": value }])], &[], UNTIL);
    assert!(
        shows.is_empty(),
        "a show without a TVmaze poster is left out"
    );
}

#[test]
fn the_list_is_fetched_while_home_shows_it_and_refreshed_later() {
    let apps = installed();
    let mut shows = Shows::default();
    assert!(shows.sync(false, true, "US", &apps, 0).0.is_none());
    assert!(shows.sync(true, true, "US", &[], 0).0.is_none());
    let (Some(command), _) = shows.sync(true, true, "US", &apps, 0) else {
        panic!("Home with the widget should fetch");
    };
    assert!(shows.loading);
    assert_eq!(command.country, "US");
    assert!(
        shows.sync(true, true, "US", &apps, 1).0.is_none(),
        "one in flight"
    );
    assert!(shows.accept(command.id, Ok(Vec::new()), 10));
    assert!(!shows.loading);
    assert!(shows.sync(true, true, "US", &apps, 11).0.is_none());
    assert!(
        shows
            .sync(true, true, "US", &apps, 10 + REFRESH_SECONDS)
            .0
            .is_some()
    );
}

#[test]
fn posters_are_wanted_once_around_the_row() {
    let mut shows = Shows {
        list: parse_schedules(
            &[json!(
                (1..=20)
                    .map(|id| json!({ "show": entry(id, "S", 100 - id, "Netflix") }))
                    .collect::<Vec<_>>()
            )],
            &installed(),
            UNTIL,
        ),
        ..Default::default()
    };
    let first = shows.wanted_posters(0);
    assert_eq!(first.len(), VISIBLE * 3);
    assert!(
        shows.wanted_posters(0).is_empty(),
        "loading ones are not asked again"
    );
    assert!(shows.accept_poster(first[0].clone(), None));
    assert!(!shows.accept_poster(first[0].clone(), None));
}

#[test]
fn schedule_entries_keep_each_episodes_air_time() {
    let web = json!([
        {"airstamp": "2026-11-08T21:00:00+00:00", "season": 2, "number": 6,
         "_embedded": {"show": entry(1, "Loud", 90, "Netflix")}},
        {"airstamp": "2026-11-01T21:00:00+00:00", "season": 2, "number": 5,
         "_embedded": {"show": entry(1, "Loud", 90, "Netflix")}},
        {"airstamp": "2026-11-01T21:00:00+00:00", "season": 2, "number": 5,
         "_embedded": {"show": entry(1, "Loud", 90, "Netflix")}},
        {"_embedded": {"show": entry(2, "Undated", 40, "Netflix")}}
    ]);
    let shows = parse_schedules(&[web], &installed(), UNTIL);
    let loud = &shows[0];
    assert_eq!(
        loud.episodes,
        [
            Episode {
                airs: 1_793_566_800,
                season: Some(2),
                number: Some(5)
            },
            Episode {
                airs: 1_793_566_800 + 7 * 86_400,
                season: Some(2),
                number: Some(6)
            },
        ]
    );
    // An entry without its air time keeps the show, with no episodes.
    assert_eq!(shows[1].title, "Undated");
    assert!(shows[1].episodes.is_empty());
}

#[test]
fn an_episode_airing_this_local_evening_counts_even_on_the_next_utc_date() {
    let tv = json!([
        {"airstamp": "2026-11-02T01:00:00+00:00", "season": 1, "number": 3,
         "show": entry(7, "Late", 50, "Netflix")}
    ]);
    let show = &parse_schedules(&[tv], &installed(), UNTIL)[0];
    // 1 November in UTC−5: 05:00Z on the 1st to 05:00Z on the 2nd.
    let day = (1_793_509_200, 1_793_509_200 + 86_400);
    assert_eq!(
        show.today(day.0, day.1, day.0).map(|e| e.number),
        Some(Some(3))
    );
    assert!(show.today(day.1, day.1 + 86_400, day.1).is_none());
}

#[test]
fn a_show_keeps_its_newest_airings() {
    // Ten daily airings at 21:00 UTC, from 23 October to tonight's of 1 November and tomorrow's.
    let tonight = 1_793_566_800;
    let web = json!(
        (0..10)
            .map(|day| {
                let airs = tonight + (day - 8) * 86_400;
                json!({
                    "airstamp": time::OffsetDateTime::from_unix_timestamp(airs)
                        .unwrap()
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap(),
                    "_embedded": {"show": entry(1, "Daily", 90, "Netflix")}
                })
            })
            .collect::<Vec<_>>()
    );
    let show = &parse_schedules(&[web], &installed(), UNTIL)[0];
    assert_eq!(show.episodes.len(), 8);
    // Tonight's and tomorrow's airings are kept, the two oldest are not.
    assert_eq!(show.episodes.last().unwrap().airs, tonight + 86_400);
    assert_eq!(show.episodes[0].airs, tonight - 6 * 86_400);
}

#[test]
fn shows_that_only_air_later_come_after_this_weeks() {
    let web = json!([
        {"airstamp": "2026-11-02T21:00:00+00:00",
         "_embedded": {"show": entry(1, "Premiere", 99, "Netflix")}},
        {"airstamp": "2026-10-31T21:00:00+00:00",
         "_embedded": {"show": entry(2, "Running", 10, "Netflix")}}
    ]);
    let titles: Vec<String> = parse_schedules(&[web], &installed(), UNTIL)
        .into_iter()
        .map(|show| show.title)
        .collect();
    assert_eq!(titles, ["Running", "Premiere"]);
}

#[test]
fn the_days_episode_is_the_coming_one_else_the_last_aired() {
    let morning = NOW - 11 * 3600;
    let evening = NOW + 3600;
    let show = Show {
        id: 1,
        title: "Twice".into(),
        poster: None,
        meta: String::new(),
        service: None,
        app: None,
        episodes: [morning, evening]
            .map(|airs| Episode {
                airs,
                season: None,
                number: None,
            })
            .to_vec(),
    };
    let day = (NOW - 20 * 3600, NOW + 4 * 3600);
    assert_eq!(
        show.today(day.0, day.1, UNTIL).map(|e| e.airs),
        Some(evening)
    );
    assert_eq!(
        show.today(day.0, day.1, evening + 60).map(|e| e.airs),
        Some(evening),
        "after both, the latest"
    );
}
