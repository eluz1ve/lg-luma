use super::*;
use serde_json::json;
use time::{Date, Month};

#[test]
fn a_device_code_becomes_a_login_to_show() {
    let reply = json!({
        "device_code": "cub_dc_abc",
        "user_code": "WDJB-MJHT",
        "verification_uri": "https://cub.red/activate",
        "verification_uri_complete": "https://cub.red/activate?code=WDJB-MJHT",
        "expires_in": 600,
        "interval": 5
    });
    let login = parse_login(&reply, 1_000).unwrap();
    assert_eq!(login.device_code, "cub_dc_abc");
    assert_eq!(login.user_code, "WDJB-MJHT");
    assert_eq!(login.uri, "cub.red/activate");
    assert_eq!(login.link, "https://cub.red/activate?code=WDJB-MJHT");
    assert_eq!((login.interval, login.expires), (5, 1_600));
    assert!(parse_login(&json!({"user_code": "X"}), 0).is_none());
}

#[test]
fn the_token_poll_answers_in_oauth_terms() {
    let tokens = json!({
        "access_token": "cub_at_1",
        "token_type": "Bearer",
        "expires_in": 86400,
        "refresh_token": "cub_rt_1"
    });
    assert_eq!(
        parse_grant(&tokens, 100),
        Grant::Tokens(Tokens {
            access: "cub_at_1".into(),
            refresh: "cub_rt_1".into(),
            expires: 86_500,
        })
    );
    let error = |code: &str| parse_grant(&json!({"error": code}), 0);
    assert_eq!(error("authorization_pending"), Grant::Pending);
    assert_eq!(error("slow_down"), Grant::SlowDown);
    assert_eq!(error("access_denied"), Grant::Denied);
    assert_eq!(error("expired_token"), Grant::Expired);
    assert_eq!(error("invalid_grant"), Grant::Revoked);
    assert!(matches!(error("server_error"), Grant::Failed(_)));
}

#[test]
fn the_main_profile_is_the_one_read() {
    let reply = json!({"secuses": true, "profiles": [
        {"id": 11, "name": "Дети", "main": 0},
        {"id": 7, "name": "Илья", "main": 1}
    ]});
    assert_eq!(
        parse_profile(&reply),
        Some(Profile {
            id: "7".into(),
            name: "Илья".into()
        })
    );
    let first = json!({"profiles": [{"id": "3", "name": "A"}, {"id": "4", "name": "B"}]});
    assert_eq!(parse_profile(&first).map(|p| p.id), Some("3".into()));
    assert_eq!(parse_profile(&json!({"profiles": []})), None);
}

#[test]
fn the_calendar_lists_releases_of_bookmarked_cards() {
    let reply = json!({
        "secuses": true,
        "items": [
            {
                "date": "2026-11-03",
                "card": {"id": 2190, "type": "tv", "name": "Южный Парк",
                         "original_name": "South Park", "poster_path": "/1CGw.jpg"},
                "season": 29,
                "episodes": [{"episode": 4, "name": "Эпизод 4"}, {"episode": 5, "name": "Эпизод 5"}],
                "watched": false,
                "badge": "finale"
            },
            {
                "date": "2026-10-29",
                "card": {"id": 1401459, "type": "movie", "name": "Фьорд", "poster_path": null},
                "season": 0,
                "episodes": [],
                "watched": true,
                "badge": "movie"
            },
            {"date": "someday", "card": {"id": 1, "type": "tv", "name": "Broken"}}
        ]
    });
    let releases = parse_calendar(&reply);
    assert_eq!(releases.len(), 2);
    assert_eq!(
        releases[0],
        Release {
            card: "tv_2190".into(),
            title: "Южный Парк".into(),
            poster: Some("/1CGw.jpg".into()),
            date: Date::from_calendar_date(2026, Month::November, 3).unwrap(),
            season: 29,
            episodes: vec![4, 5],
            badge: Badge::Finale,
            watched: false,
        }
    );
    assert_eq!(releases[1].card, "movie_1401459");
    assert_eq!(releases[1].poster, None);
    assert_eq!(releases[1].badge, Badge::Movie);
    assert!(releases[1].watched);
}

#[test]
fn the_history_gives_each_card_its_last_episode() {
    let reply = json!({
        "secuses": true,
        "history": [
            {
                "card_id": "tv_1399", "card_type": "tv",
                "watched_at": 1_792_762_158_824i64,
                "last": {"season": 2, "episode": 1, "watched_at": 1_792_762_158_824i64},
                "card": {"id": 1399, "name": "Игра престолов", "poster_path": "/1XS1.jpg"}
            },
            {
                "card_id": "movie_550", "card_type": "movie",
                "watched_at": 1_792_000_000_000i64,
                "card": {"id": 550, "name": "Бойцовский клуб"}
            }
        ]
    });
    let watched = parse_history(&reply);
    assert_eq!(
        watched[0],
        Watched {
            card: "tv_1399".into(),
            title: "Игра престолов".into(),
            poster: Some("/1XS1.jpg".into()),
            episode: Some((2, 1)),
            at: 1_792_762_158,
        }
    );
    assert_eq!(watched[1].card, "movie_550");
    assert_eq!(watched[1].episode, None);
}

#[test]
fn posters_come_from_tmdb_at_card_size() {
    assert_eq!(
        poster_urls("/1XS1.jpg"),
        [
            "https://image.tmdb.org/t/p/w185/1XS1.jpg",
            "https://imagetmdb.com/t/p/w185/1XS1.jpg"
        ]
    );
    assert!(poster_urls("../x.jpg").is_empty());
    assert!(poster_urls("/a b.jpg").is_empty());
}

fn tokens(expires: i64) -> Tokens {
    Tokens {
        access: "a".into(),
        refresh: "r".into(),
        expires,
    }
}

#[test]
fn tokens_are_renewed_an_hour_before_they_lapse() {
    assert!(!needs_refresh(&tokens(10_000), 6_000));
    assert!(needs_refresh(&tokens(10_000), 6_401));
}

#[test]
fn the_calendar_is_read_from_a_week_back_to_tomorrow() {
    let today = Date::from_calendar_date(2026, Month::November, 3).unwrap();
    assert_eq!(
        range(today),
        (
            Date::from_calendar_date(2026, Month::October, 27).unwrap(),
            Date::from_calendar_date(2026, Month::November, 4).unwrap()
        )
    );
}

fn login(code: &str) -> Login {
    Login {
        device_code: code.into(),
        user_code: "ABCD-EFGH".into(),
        uri: "cub.red/activate".into(),
        link: "https://cub.red/activate?code=ABCD-EFGH".into(),
        interval: 5,
        expires: 1_600,
    }
}

#[test]
fn a_sign_in_polls_at_its_interval_until_confirmed() {
    let mut cub = Cub::default();
    assert_eq!(cub.due(1_000), None);
    cub.begin();
    assert!(cub.signing_in);
    assert_eq!(cub.due(1_000), Some(Due::SignIn));
    assert_eq!(cub.due(1_001), None, "one code at a time");
    cub.login(Ok(login("dc")), 1_000);
    assert_eq!(cub.due(1_004), None);
    assert_eq!(cub.due(1_005), Some(Due::Poll("dc".into())));
    // The poll is in flight until its answer.
    assert_eq!(cub.due(1_006), None);
    assert!(cub.grant("dc", Grant::SlowDown, 1_006).is_none());
    assert_eq!(cub.due(1_015), None);
    assert_eq!(cub.due(1_016), Some(Due::Poll("dc".into())));
    // A stale code's answer is ignored.
    assert!(
        cub.grant("old", Grant::Tokens(tokens(99_999)), 1_017)
            .is_none()
    );
    let saved = cub.grant("dc", Grant::Tokens(tokens(99_999)), 1_017);
    assert_eq!(saved, Some(tokens(99_999)));
    assert!(cub.login.is_none() && !cub.signing_in);
    assert!(cub.signed_in());
    assert!(matches!(cub.due(1_017), Some(Due::Read { .. })));
}

#[test]
fn an_expired_or_refused_code_ends_the_sign_in() {
    for grant in [Grant::Expired, Grant::Denied] {
        let mut cub = Cub::default();
        cub.begin();
        cub.login(Ok(login("dc")), 1_000);
        let _ = cub.due(1_005);
        cub.grant("dc", grant, 1_005);
        assert!(cub.login.is_none() && !cub.signing_in);
        assert!(cub.failed);
        assert!(!cub.signed_in());
    }
    // Past its expiry the code is not polled again.
    let mut cub = Cub::default();
    cub.begin();
    cub.login(Ok(login("dc")), 1_000);
    assert_eq!(cub.due(1_601), None);
    assert!(cub.login.is_none() && cub.failed);
}

#[test]
fn the_account_is_read_now_and_then_and_sooner_after_a_failure() {
    let mut cub = Cub::default();
    cub.sign_in_with(tokens(99_999), None);
    let Some(Due::Read { id, .. }) = cub.due(1_000) else {
        panic!("reads at once");
    };
    assert_eq!(cub.due(1_001), None, "one read at a time");
    let account = Account {
        profile: Profile {
            id: "7".into(),
            name: "Илья".into(),
        },
        releases: vec![],
        watched: vec![],
    };
    cub.account(id, None, Ok(account.clone()), 1_002);
    assert_eq!(cub.profile.as_ref().map(|p| p.name.as_str()), Some("Илья"));
    assert_eq!(cub.due(1_002 + REFRESH - 1), None);
    let Some(Due::Read { id, .. }) = cub.due(1_002 + REFRESH) else {
        panic!("reads again");
    };
    cub.account(id, None, Err(Failure::Unavailable("x".into())), 5_000);
    assert!(cub.failed);
    assert!(matches!(cub.due(5_000 + RETRY), Some(Due::Read { .. })));
}

#[test]
fn renewed_tokens_are_kept_and_a_dead_sign_in_signs_out() {
    let mut cub = Cub::default();
    cub.sign_in_with(tokens(10), None);
    let Some(Due::Read { id, .. }) = cub.due(1_000) else {
        panic!();
    };
    let save = cub.account(
        id,
        Some(tokens(90_000)),
        Err(Failure::Unavailable("x".into())),
        1_001,
    );
    assert_eq!(save, Saved::Tokens);
    assert_eq!(cub.tokens, Some(tokens(90_000)));
    let Some(Due::Read { id, .. }) = cub.due(1_001 + RETRY) else {
        panic!();
    };
    let save = cub.account(id, None, Err(Failure::SignedOut), 2_000);
    assert_eq!(save, Saved::Forget);
    assert!(!cub.signed_in());
    assert_eq!(cub.due(9_999), None);
}

#[test]
fn a_failed_code_request_can_be_asked_again() {
    let mut cub = Cub::default();
    cub.begin();
    let _ = cub.due(1_000);
    cub.login(Err("offline".into()), 1_000);
    assert!(cub.failed && !cub.signing_in);
    cub.begin();
    assert_eq!(cub.due(1_001), Some(Due::SignIn));
}

#[test]
fn the_sign_in_is_kept_privately_and_forgotten() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!("luma-cub-{}", std::process::id()));
    let path = directory.join("cub.json");
    assert_eq!(load(&path), None);
    let profile = Profile {
        id: "7".into(),
        name: "Илья".into(),
    };
    save(&path, &tokens(99_999), Some(&profile)).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(load(&path), Some((tokens(99_999), Some(profile))));
    forget(&path);
    assert_eq!(load(&path), None);
    std::fs::write(&path, b"{not json").unwrap();
    assert_eq!(load(&path), None);
    let _ = std::fs::remove_dir_all(&directory);
}

fn release(card: &str, poster: Option<&str>, watched: bool) -> Release {
    Release {
        card: card.into(),
        title: card.into(),
        poster: poster.map(Into::into),
        date: Date::from_calendar_date(2026, Month::November, 3).unwrap(),
        season: 1,
        episodes: vec![1],
        badge: Badge::None,
        watched,
    }
}

#[test]
fn posters_are_fetched_once_for_what_today_may_show() {
    let mut cub = Cub::default();
    cub.releases = vec![
        release("tv_1", Some("/a.jpg"), false),
        release("tv_2", Some("/b.jpg"), true),
        release("tv_3", None, false),
    ];
    cub.watched = vec![Watched {
        card: "tv_4".into(),
        title: "Four".into(),
        poster: Some("/c.jpg".into()),
        episode: Some((1, 2)),
        at: 1_000,
    }];
    assert_eq!(cub.wanted_posters(), ["/a.jpg", "/c.jpg"]);
    assert!(cub.wanted_posters().is_empty(), "asked once");
    assert!(cub.poster("/a.jpg").is_none());
    assert!(!cub.accept_poster("/zzz.jpg".into(), None), "never asked");
    assert!(cub.accept_poster("/a.jpg".into(), None));
    assert!(cub.poster("/a.jpg").is_none());
}

#[test]
fn coming_back_to_luma_reads_again_unless_just_read() {
    let mut cub = Cub::default();
    cub.shown(true, 900);
    assert_eq!(cub.due(1_000), None, "not signed in");
    cub.sign_in_with(tokens(99_999), None);
    let Some(Due::Read { id, .. }) = cub.due(1_000) else {
        panic!("reads at once");
    };
    let account = Account {
        profile: Profile {
            id: "7".into(),
            name: "Илья".into(),
        },
        releases: vec![],
        watched: vec![],
    };
    cub.account(id, None, Ok(account), 1_000);
    // Off to Lampa and back within a couple of minutes: what was read still holds.
    cub.shown(false, 1_010);
    cub.shown(true, 1_000 + FRESH - 1);
    assert_eq!(cub.due(1_000 + FRESH - 1), None);
    // Back later: read now, not at the half-hour.
    cub.shown(false, 1_000 + FRESH);
    assert_eq!(cub.due(1_000 + FRESH), None, "hidden, nothing new");
    cub.shown(true, 1_000 + FRESH);
    assert!(matches!(cub.due(1_000 + FRESH), Some(Due::Read { .. })));
}
