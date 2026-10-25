use super::*;
use serde_json::json;

#[test]
fn quotes_carry_price_change_and_range_in_coin_order() {
    let body = json!([
        {"symbol": "ETHUSDT", "openPrice": "2400.0", "highPrice": "2450.0", "lowPrice": "2280.0",
         "lastPrice": "2300.0"},
        {"symbol": "BTCUSDT", "openPrice": "80000.00", "highPrice": "81000.00",
         "lowPrice": "79000.00", "lastPrice": "80800.00"},
        {"symbol": "XYZUSDT", "openPrice": "1", "highPrice": "1", "lowPrice": "1", "lastPrice": "1"}
    ]);
    let quotes = parse_quotes(&body).unwrap();
    assert_eq!(
        quotes.iter().map(|q| q.coin).collect::<Vec<_>>(),
        ["BTC", "ETH"]
    );
    assert!((quotes[0].change - 1.0).abs() < 1e-9);
    assert!((quotes[1].change - (-100. / 24.)).abs() < 1e-9);
    assert_eq!((quotes[0].low, quotes[0].high), (79000., 81000.));
    assert!(parse_quotes(&json!({"code": 0})).is_err());
}

fn episode(name: &str, season: u32, number: u32, date: &str, kind: &str, weight: u32) -> Value {
    json!({
        "season": season, "number": number, "airdate": date,
        "_embedded": {"show": {
            "id": weight * 1000 + season, "name": name, "type": kind, "weight": weight,
            "image": {"medium": format!("https://static.tvmaze.com/{name}.jpg")}
        }}
    })
}

#[test]
fn premieres_keep_first_episodes_of_popular_scripted_shows_by_date() {
    let web = json!([
        episode("Lupin", 4, 1, "2026-10-23", "Scripted", 100),
        episode("Lupin", 4, 2, "2026-10-23", "Scripted", 100),
        episode("Love Island", 8, 1, "2026-10-22", "Reality", 99),
        episode("Niche", 2, 1, "2026-10-22", "Scripted", 60),
    ]);
    // The US schedule embeds the show under "show" instead of "_embedded".
    let us = json!([
        {"season": 2, "number": 1, "airdate": "2026-10-21",
         "show": {"id": 7, "name": "Stick", "type": "Animation", "weight": 99, "image": null}},
        episode("Lupin", 4, 1, "2026-10-23", "Scripted", 100),
    ]);
    let premieres = parse_premieres(&[web, us]);
    assert_eq!(
        premieres
            .iter()
            .map(|p| (p.title.as_str(), p.season, p.date.as_str()))
            .collect::<Vec<_>>(),
        [("Stick", 2, "2026-10-21"), ("Lupin", 4, "2026-10-23")]
    );
    assert_eq!(premieres[0].poster, None);
    assert_eq!(
        premieres[1].poster.as_deref(),
        Some("https://static.tvmaze.com/Lupin.jpg")
    );
}

#[test]
fn posters_only_come_from_the_tvmaze_image_origin() {
    assert!(poster_allowed(
        "https://static.tvmaze.com/uploads/images/medium_portrait/1/2.jpg"
    ));
    assert!(!poster_allowed("http://static.tvmaze.com/a.jpg"));
    assert!(!poster_allowed(
        "https://static.tvmaze.com.evil.example/a.jpg"
    ));
    assert!(!poster_allowed("https://example.com/a.jpg"));
}

#[test]
fn schedule_days_run_from_three_days_ago_to_ten_ahead() {
    let today = time::Date::from_calendar_date(2026, time::Month::October, 25).unwrap();
    let days = schedule_days(today);
    assert_eq!(days.len(), 14);
    assert_eq!(days.first().map(String::as_str), Some("2026-10-22"));
    assert_eq!(days.last().map(String::as_str), Some("2026-11-04"));
}

fn cs_match(id: u64, tier: &str, begin: &str, teams: [(u64, &str, &str); 2]) -> Value {
    json!({
        "id": id, "status": "not_started", "begin_at": begin, "number_of_games": 3,
        "opponents": teams.map(|(id, name, acronym)| json!({
            "type": "Team", "opponent": {"id": id, "name": name, "acronym": acronym}
        })),
        "results": [{"team_id": teams[0].0, "score": 0}, {"team_id": teams[1].0, "score": 0}],
        "games": [],
        "league": {"name": "BLAST"}, "serie": {"full_name": "Premier Fall 2026"},
        "tournament": {"name": "Playoffs", "tier": tier}
    })
}

#[test]
fn matches_carry_teams_series_score_and_maps() {
    let mut live = cs_match(
        7,
        "s",
        "2026-10-25T18:00:00Z",
        [(1, "Natus Vincere", "NAVI"), (2, "FaZe Clan", "FaZe")],
    );
    live["status"] = json!("running");
    live["results"] = json!([{"team_id": 2, "score": 0}, {"team_id": 1, "score": 1}]);
    live["games"] = json!([
        {"position": 2, "status": "running", "winner": {"id": null, "type": "Team"}},
        {"position": 1, "status": "finished", "winner": {"id": 1, "type": "Team"}},
        {"position": 3, "status": "not_started", "winner": {"id": null}}
    ]);
    let matches = parse_matches(&json!([live]));
    let found = &matches[0];
    assert!(found.running);
    assert_eq!(found.teams[0].as_ref().unwrap().short, "NAVI");
    assert_eq!(found.score, [1, 0], "scores follow the opponents' order");
    assert_eq!(found.best_of, 3);
    assert_eq!(found.event, "BLAST Premier Fall 2026");
    assert_eq!(
        found
            .games
            .iter()
            .map(|g| (g.position, g.state, g.winner))
            .collect::<Vec<_>>(),
        [
            (1, GameState::Finished, Some(1)),
            (2, GameState::Running, None),
            (3, GameState::Waiting, None)
        ]
    );
    // A team still to be decided is left empty rather than dropping the match.
    let mut open = cs_match(
        8,
        "a",
        "2026-10-26T12:00:00Z",
        [(3, "Vitality", "VIT"), (4, "G2", "G2")],
    );
    open["opponents"] = json!([open["opponents"][0].clone()]);
    let open = &parse_matches(&json!([open]))[0];
    assert!(open.teams[1].is_none());
}

#[test]
fn major_only_keeps_s_and_a_tier_events_with_live_matches_first() {
    let upcoming = parse_matches(&json!([
        cs_match(
            1,
            "c",
            "2026-10-25T19:00:00Z",
            [(1, "A", "A"), (2, "B", "B")]
        ),
        cs_match(
            2,
            "a",
            "2026-10-26T10:00:00Z",
            [(3, "C", "C"), (4, "D", "D")]
        ),
        cs_match(
            3,
            "s",
            "2026-10-25T21:00:00Z",
            [(5, "E", "E"), (6, "F", "F")]
        ),
    ]));
    let mut live = cs_match(
        4,
        "b",
        "2026-10-25T17:00:00Z",
        [(7, "G", "G"), (8, "H", "H")],
    );
    live["status"] = json!("running");
    let running = parse_matches(&json!([live]));
    let ids = |matches: Vec<CsMatch>| matches.iter().map(|m| m.id).collect::<Vec<_>>();
    assert_eq!(
        ids(select_matches(running.clone(), upcoming.clone(), true)),
        [3, 2]
    );
    assert_eq!(ids(select_matches(running, upcoming, false)), [4, 1, 3, 2]);
}

#[test]
fn unknown_disciplines_fall_back_to_counter_strike() {
    assert_eq!(game_path("dota2"), "dota2");
    assert_eq!(game_path("../admin"), "csgo");
    assert_eq!(GAMES[0].0, "csgo");
}

#[test]
fn rosters_list_each_teams_players_by_nickname() {
    let teams = json!([
        {"id": 1, "players": [{"name": "s1mple"}, {"name": "b1t "}, {"first_name": "x"}]},
        {"id": 2, "players": []}
    ]);
    assert_eq!(
        parse_rosters(&teams),
        vec![
            (1, vec!["s1mple".to_owned(), "b1t".to_owned()]),
            (2, vec![])
        ]
    );
}

#[test]
fn the_shared_feed_carries_matches_and_their_lineups() {
    let body = json!({"game": "csgo", "updated": 1, "matches": [
        {"id": 5, "running": true, "begin_at": "2026-10-25T11:11:51Z",
         "teams": [{"id": 1, "name": "Acend", "short": "ACE"}, null],
         "score": [1, 0], "best_of": 3,
         "games": [{"position": 1, "state": "finished", "winner": 1},
                   {"position": 2, "state": "running", "winner": null}],
         "event": "CCT Europe", "tier": "c",
         "lineups": [["SHiPZ", "SPELLAN"], []]},
        {"id": "broken"}
    ]});
    let (matches, rosters) = parse_feed(&body, "csgo");
    assert_eq!(matches.len(), 1);
    let found = &matches[0];
    assert_eq!(
        (found.id, found.game.as_str(), found.running),
        (5, "csgo", true)
    );
    assert_eq!(found.teams[0].as_ref().unwrap().short, "ACE");
    assert!(found.teams[1].is_none());
    assert_eq!(found.games[1].state, GameState::Running);
    assert_eq!(
        rosters,
        vec![(1, vec!["SHiPZ".to_owned(), "SPELLAN".to_owned()])]
    );
    assert!(feed_url("csgo").starts_with("https://eluz1ve.github.io/luma-feeds/esports/"));
    assert!(feed_url("../x").ends_with("/csgo.json"));
}

#[test]
fn a_personal_key_polls_live_matches_as_often_as_the_free_limit_allows() {
    assert_eq!(live_interval(1), 30);
    assert_eq!(live_interval(2), 30);
    assert_eq!(
        live_interval(8),
        64,
        "8 disciplines × 2 lists stay under 1000 requests an hour"
    );
}
