use super::*;
use serde_json::json;

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
    let shows = parse_schedules(&[web, tv], &installed());
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
    let shows = parse_schedules(&[json!([{ "show": value }])], &[]);
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
