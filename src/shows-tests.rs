use super::*;
use serde_json::json;

/// 4 November 2026, the TV's date.
const DATE: Date = match Date::from_calendar_date(2026, time::Month::November, 4) {
    Ok(date) => date,
    Err(_) => panic!(),
};

/// A series of CUB's feed whose latest episode came out on `aired`.
fn series(id: i64, name: &str, aired: &str) -> Value {
    json!({
        "id": id, "name": name, "original_name": format!("{name} (original)"),
        "poster_path": format!("/{id}.jpg"), "last_air_date": aired,
        "seasons": {"1": 10, "2": 7}, "vote_average": 8.615, "vote_count": 120
    })
}

fn page(results: Vec<Value>) -> Value {
    json!({"page": 1, "results": results})
}

#[test]
fn the_feed_keeps_series_with_a_new_episode_in_its_order() {
    let pages = [
        page(vec![
            series(1, "Гангстерленд", "2026-10-29"),
            series(2, "Во все тяжкие", "2021-11-22"),
            series(3, "Ноктюрн", "2026-11-03"),
        ]),
        page(vec![
            series(3, "Ноктюрн", "2026-11-03"),
            series(4, "Три недели", "2026-10-14"),
            series(5, "Давно", "2026-10-13"),
        ]),
    ];
    let shows = parse_feed(&pages, DATE);
    let ids: Vec<i64> = shows.iter().map(|show| show.id).collect();
    assert_eq!(ids, [1, 3, 4], "old ones and repeats left out");
    let first = &shows[0];
    assert_eq!(first.name, "Гангстерленд");
    assert_eq!(first.original, "Гангстерленд (original)");
    assert_eq!(first.poster.as_deref(), Some("/1.jpg"));
    assert_eq!(first.season, Some(2), "the latest season");
    assert_eq!(first.rating, Some(8.6));
    assert_eq!(
        first.aired,
        Date::from_calendar_date(2026, time::Month::October, 29).unwrap()
    );
}

#[test]
fn series_without_a_poster_or_a_date_are_left_out_and_the_rest_need_little() {
    let mut no_poster = series(1, "A", "2026-11-01");
    no_poster["poster_path"] = json!("../a.jpg");
    let mut no_date = series(2, "B", "2026-11-01");
    no_date["last_air_date"] = json!(null);
    let mut bare = series(3, "C", "2026-11-01");
    bare["seasons"] = json!(null);
    bare["vote_count"] = json!(3);
    let mut no_original = series(4, "D", "2026-11-01");
    no_original["original_name"] = json!("");
    // Dated after tomorrow: not out yet.
    let later = series(5, "E", "2026-11-06");
    let shows = parse_feed(
        &[page(vec![no_poster, no_date, bare, no_original, later])],
        DATE,
    );
    let ids: Vec<i64> = shows.iter().map(|show| show.id).collect();
    assert_eq!(ids, [3, 4]);
    assert_eq!(shows[0].season, None);
    assert_eq!(shows[0].rating, None, "too few votes");
    assert_eq!(shows[1].original, "D", "its name stands in");
}

#[test]
fn the_feed_lists_at_most_a_row_and_a_half() {
    let results = (1..=40)
        .map(|id| series(id, "S", "2026-11-01"))
        .collect::<Vec<_>>();
    assert_eq!(parse_feed(&[page(results)], DATE).len(), SHOWN);
}

#[test]
fn russian_titles_show_in_russian_and_the_original_elsewhere() {
    let show = &parse_feed(&[page(vec![series(1, "Люпен", "2026-11-01")])], DATE)[0];
    assert_eq!(show.title(crate::i18n::Language::Ru), "Люпен");
    assert_eq!(show.title(crate::i18n::Language::En), "Люпен (original)");
}

#[test]
fn an_episode_dated_yesterday_is_new_today_too() {
    let show = &parse_feed(&[page(vec![series(1, "S", "2026-11-03")])], DATE)[0];
    assert!(show.new_on(DATE));
    assert!(show.new_on(DATE.previous_day().unwrap()));
    assert!(!show.new_on(DATE.next_day().unwrap().next_day().unwrap()));
}

#[test]
fn the_list_is_fetched_while_home_shows_it_and_refreshed_later() {
    let mut shows = Shows::default();
    assert!(shows.sync(false, true, 0).0.is_none());
    assert!(shows.sync(true, false, 0).0.is_none());
    let (Some(command), _) = shows.sync(true, true, 0) else {
        panic!("Home with the widget should fetch");
    };
    assert!(shows.loading);
    assert!(shows.sync(true, true, 1).0.is_none(), "one in flight");
    assert!(shows.accept(command.id, Ok(Vec::new()), 10));
    assert!(!shows.loading);
    assert!(shows.sync(true, true, 11).0.is_none());
    assert!(shows.sync(true, true, 10 + REFRESH_SECONDS).0.is_some());
}

#[test]
fn a_failed_fetch_is_retried_sooner() {
    let mut shows = Shows::default();
    let (Some(command), _) = shows.sync(true, true, 0) else {
        panic!("a fetch is due");
    };
    assert!(shows.accept(command.id, Err("offline".into()), 10));
    assert!(shows.failed);
    assert!(shows.sync(true, true, 10 + RETRY_SECONDS).0.is_some());
}

#[test]
fn posters_are_wanted_once_around_the_row() {
    let results = (1..=20)
        .map(|id| series(id, "S", "2026-11-01"))
        .collect::<Vec<_>>();
    let mut shows = Shows {
        list: parse_feed(&[page(results)], DATE),
        ..Default::default()
    };
    let first = shows.wanted_posters(0);
    assert_eq!(first.len(), SHOWN.min(VISIBLE * 3));
    assert_eq!(first[0], "/1.jpg");
    assert!(
        shows.wanted_posters(0).is_empty(),
        "loading ones are not asked again"
    );
    assert!(shows.accept_poster(first[0].clone(), None));
    assert!(!shows.accept_poster(first[0].clone(), None));
}

#[test]
fn posters_come_from_tmdb_or_its_mirror_only() {
    assert_eq!(
        poster_urls("/1.jpg"),
        [
            "https://image.tmdb.org/t/p/w342/1.jpg",
            "https://imagetmdb.com/t/p/w342/1.jpg"
        ]
    );
    assert!(poster_urls("https://example.com/a.jpg").is_empty());
}
