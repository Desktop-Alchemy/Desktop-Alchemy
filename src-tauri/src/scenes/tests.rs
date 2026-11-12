use super::*;
#[tokio::test]
async fn manager_deduplicates_page_changes_handles_fallback_and_stops() {
    let core = Core::new_test_device(
        std::env::temp_dir().join(format!("m18-scene-manager-{}", uuid::Uuid::new_v4())),
    )
    .unwrap();
    let base = core.snapshot().active_page;
    let page = crate::devices::default_page("自动页");
    let mut pages = core.snapshot().config.pages;
    pages.push(page.clone());
    core.update_pages(pages).unwrap();
    let rule = Scene {
        page_id: page.id.clone(),
        ..scene("first", always())
    };
    core.save_scenes(settings(vec![
        rule.clone(),
        Scene {
            id: "second".into(),
            ..rule
        },
    ]))
    .unwrap();
    let manager = SceneManager::new(core.clone());
    manager.start();
    tokio::time::timeout(Duration::from_secs(3), async {
        while core.snapshot().active_page != page.id {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut s = core.snapshot().config.scene_settings;
    s.scenes.swap(0, 1);
    core.save_scenes(s.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(
        manager
            .metrics
            .switches
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(core.snapshot().config.active_page, base);
    s.scenes.iter_mut().for_each(|s| s.enabled = false);
    core.save_scenes(s).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while core.snapshot().active_page != base {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    manager.shutdown().await;
    assert_eq!(
        manager
            .metrics
            .switches
            .load(std::sync::atomic::Ordering::Relaxed),
        2
    );
    std::fs::remove_dir_all(&core.root).unwrap();
}

fn at(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
}
fn scene(id: &str, condition: SceneCondition) -> Scene {
    Scene {
        id: id.into(),
        name: id.into(),
        enabled: true,
        page_id: id.into(),
        trigger: SceneTrigger::Continuous,
        condition,
        enter_delay_ms: 0,
        exit_delay_ms: 0,
    }
}
fn settings(scenes: Vec<Scene>) -> SceneSettings {
    SceneSettings {
        enabled: true,
        scenes,
        ..Default::default()
    }
}
fn program() -> SceneCondition {
    SceneCondition::Process {
        matcher: ProcessMatcher::Name {
            value: "test.exe".into(),
        },
    }
}
fn facts() -> Facts {
    Facts {
        processes: Some(vec![RunningProgram {
            pid: 1,
            name: "TEST.EXE".into(),
            path: Some("C:\\test.exe".into()),
        }]),
        foreground: Some(None),
        media: Some(true),
    }
}
fn always() -> SceneCondition {
    SceneCondition::All { children: vec![] }
}
fn scheduled(id: &str, time: &str) -> Scene {
    Scene {
        trigger: SceneTrigger::Scheduled {
            schedule: SceneTime {
                days: DayFilter::EveryDay,
                time: time.into(),
            },
        },
        ..scene(id, always())
    }
}
#[test]
fn nested_conditions_unknown_and_exact_program_matching() {
    use ConditionTruth::*;
    let now = at("2026-11-06 09:00:00");
    let c = SceneCondition::All {
        children: vec![
            program(),
            SceneCondition::Any {
                children: vec![
                    SceneCondition::MediaPlaying,
                    SceneCondition::Foreground {
                        matcher: ProcessMatcher::Name {
                            value: "other.exe".into(),
                        },
                    },
                ],
            },
        ],
    };
    assert_eq!(evaluate(&c, &facts(), now), True);
    assert_eq!(
        evaluate(
            &SceneCondition::Not {
                child: Box::new(program())
            },
            &Facts::default(),
            now
        ),
        Unknown
    );
    let c = SceneCondition::Process {
        matcher: ProcessMatcher::Path {
            value: "c:/TEST.exe".into(),
        },
    };
    assert_eq!(evaluate(&c, &facts(), now), True);
    let mut f = facts();
    f.processes.as_mut().unwrap()[0].path = None;
    assert_eq!(evaluate(&c, &f, now), Unknown);
    f.processes.as_mut().unwrap()[0].name = "other.exe".into();
    assert_eq!(evaluate(&c, &f, now), False);
    assert_eq!(
        evaluate(
            &SceneCondition::All {
                children: vec![program(), SceneCondition::MediaPlaying]
            },
            &Facts {
                media: Some(false),
                ..Default::default()
            },
            now
        ),
        False
    );
    assert_eq!(
        evaluate(
            &SceneCondition::Any {
                children: vec![program(), SceneCondition::MediaPlaying]
            },
            &Facts {
                media: Some(true),
                ..Default::default()
            },
            now
        ),
        True
    );
}
#[test]
fn time_ranges_midnight_weekdays_and_boundaries() {
    let c = SceneCondition::TimeRange {
        days: DayFilter::Weekdays { days: vec![5] },
        start: "22:00".into(),
        end: "02:00".into(),
    };
    assert_eq!(
        evaluate(&c, &Facts::default(), at("2026-11-06 22:00:00")),
        ConditionTruth::True
    );
    assert_eq!(
        evaluate(&c, &Facts::default(), at("2026-11-07 01:59:59")),
        ConditionTruth::True
    );
    assert_eq!(
        evaluate(&c, &Facts::default(), at("2026-11-07 02:00:00")),
        ConditionTruth::False
    );
    assert_eq!(
        evaluate(&c, &Facts::default(), at("2026-11-07 22:00:00")),
        ConditionTruth::False
    );
    let c = SceneCondition::TimeRange {
        days: DayFilter::Date {
            date: "2026-11-06".into(),
        },
        start: "09:00".into(),
        end: "10:00".into(),
    };
    assert_eq!(
        evaluate(&c, &facts(), at("2026-11-06 09:30:00")),
        ConditionTruth::True
    );
}
#[test]
fn priority_preemption_and_all_fallbacks() {
    let now = at("2026-11-06 09:00:00");
    for (fallback, target) in [
        (SceneFallback::Keep, None),
        (SceneFallback::Return, Some("base")),
        (
            SceneFallback::Page {
                page_id: "default".into(),
            },
            Some("default"),
        ),
    ] {
        let s = SceneSettings {
            fallback,
            ..settings(vec![
                scene("media", SceneCondition::MediaPlaying),
                scene("program", program()),
            ])
        };
        let mut a = Arbiter::default();
        a.reset("base".into(), now);
        let (page, status, _) = a.tick(&s, &facts(), now, 0);
        assert_eq!(page.as_deref(), Some("media"));
        assert_eq!(status.states[1].status, "covered");
        let mut f = facts();
        f.media = Some(false);
        assert_eq!(a.tick(&s, &f, now, 1).0.as_deref(), Some("program"));
        f.processes = Some(vec![]);
        assert_eq!(a.tick(&s, &f, now, 2).0.as_deref(), target);
        // Stable selection is safe for retries; the manager uploads only on target change.
        assert_eq!(a.tick(&s, &f, now, 3).0.as_deref(), target);
    }
}
#[test]
fn debounce_deadlines_unknown_immediately_releases_and_edits_reset_stability() {
    let now = at("2026-11-06 09:00:00");
    let mut s = settings(vec![scene("program", program())]);
    s.scenes[0].enter_delay_ms = 300;
    s.scenes[0].exit_delay_ms = 1000;
    let mut a = Arbiter::default();
    a.reset("base".into(), now);
    assert_eq!(a.tick(&s, &facts(), now, 0).2, Some(300));
    assert!(a.tick(&s, &facts(), now, 299).0.is_none());
    assert_eq!(a.tick(&s, &facts(), now, 300).0.as_deref(), Some("program"));
    let f = Facts {
        processes: Some(vec![]),
        ..facts()
    };
    assert_eq!(a.tick(&s, &f, now, 400).0.as_deref(), Some("program"));
    assert_eq!(a.tick(&s, &f, now, 1400).0.as_deref(), Some("base"));
    a.tick(&s, &facts(), now, 1500);
    a.tick(&s, &facts(), now, 1800);
    assert_eq!(
        a.tick(&s, &Facts::default(), now, 1801).0.as_deref(),
        Some("base")
    );
    let old = s.clone();
    s.scenes[0].condition = SceneCondition::MediaPlaying;
    a.reconfigure(&old, &s);
    assert_ne!(
        a.tick(&s, &facts(), now, 1802).0.as_deref(),
        Some("program")
    );
}
#[test]
fn scheduled_retention_priority_replacement_and_guard_once() {
    let mut guarded = scheduled("guarded", "09:32");
    guarded.condition = SceneCondition::MediaPlaying;
    let s = settings(vec![
        scene("program", program()),
        scheduled("first", "09:30"),
        scheduled("same-minute-low", "09:30"),
        scheduled("second", "09:31"),
        guarded,
    ]);
    let mut a = Arbiter::default();
    a.reset("base".into(), at("2026-11-06 09:29:59"));
    let (_, status, _) = a.tick(&s, &facts(), at("2026-11-06 09:30:00"), 1000);
    assert_eq!(status.retained_id.as_deref(), Some("first"));
    assert_eq!(status.winner_id.as_deref(), Some("program"));
    let f = Facts {
        processes: Some(vec![]),
        ..facts()
    };
    assert_eq!(
        a.tick(&s, &f, at("2026-11-06 09:30:10"), 11000)
            .0
            .as_deref(),
        Some("first")
    );
    assert_eq!(
        a.tick(&s, &f, at("2026-11-06 09:31:00"), 61000)
            .0
            .as_deref(),
        Some("second")
    );
    assert_eq!(
        a.tick(&s, &f, at("2026-11-06 09:32:00"), 121000)
            .0
            .as_deref(),
        Some("guarded")
    );
    assert_eq!(
        a.tick(
            &s,
            &Facts {
                media: Some(false),
                ..f
            },
            at("2026-11-06 09:32:01"),
            122000
        )
        .0
        .as_deref(),
        Some("guarded")
    );
    a.reset("manual".into(), at("2026-11-06 09:29:59"));
    assert!(a
        .tick(&s, &Facts::default(), at("2026-11-06 09:30:00"), 123000)
        .1
        .retained_id
        .is_none());
}
#[test]
fn schedule_does_not_replay_startup_suspend_clock_rewind_or_failed_guard() {
    let mut rule = scheduled("a", "09:30");
    rule.condition = SceneCondition::MediaPlaying;
    let s = settings(vec![rule]);
    let mut a = Arbiter::default();
    a.reset("base".into(), at("2026-11-06 09:30:20"));
    assert!(a
        .tick(&s, &facts(), at("2026-11-06 09:30:21"), 0)
        .0
        .is_none());
    a.reset("base".into(), at("2026-11-06 09:29:59"));
    assert!(a
        .tick(&s, &facts(), at("2026-11-06 10:00:00"), 0)
        .0
        .is_none());
    a.reset("base".into(), at("2026-11-06 09:29:59"));
    assert!(a
        .tick(
            &s,
            &Facts {
                media: Some(false),
                ..facts()
            },
            at("2026-11-06 09:30:00"),
            0
        )
        .0
        .is_none());
    assert!(a
        .tick(&s, &facts(), at("2026-11-06 09:30:01"), 1000)
        .0
        .is_none());
    a.reset("base".into(), at("2026-11-06 09:29:59"));
    assert!(a
        .tick(&s, &facts(), at("2026-11-06 09:30:00"), 2000)
        .0
        .is_none());
    a.reset("base".into(), at("2026-11-07 09:29:59"));
    assert_eq!(
        a.tick(&s, &facts(), at("2026-11-07 09:30:00"), 3000)
            .0
            .as_deref(),
        Some("a")
    );
}
#[test]
fn validation_legacy_defaults_and_disabled_missing_targets() {
    let config = crate::devices::default_config();
    let mut json = serde_json::to_value(&config).unwrap();
    json.as_object_mut().unwrap().remove("scene_settings");
    let legacy: crate::model::AppConfig = serde_json::from_value(json).unwrap();
    assert!(!legacy.scene_settings.enabled);
    let mut s = settings(vec![scene("missing", always())]);
    assert!(validate(&s, &config.pages).is_err());
    s.scenes[0].enabled = false;
    assert!(validate(&s, &config.pages).is_ok());
    s.scenes[0].condition = SceneCondition::Any { children: vec![] };
    assert!(validate(&s, &config.pages).is_err());
    let mut deep = always();
    for _ in 0..8 {
        deep = SceneCondition::Not {
            child: Box::new(deep),
        };
    }
    s.scenes[0].condition = deep;
    assert!(validate(&s, &config.pages).is_err());
    s.scenes[0].condition = SceneCondition::All {
        children: vec![always(); 128],
    };
    assert!(validate(&s, &config.pages).is_err());
}
#[test]
fn clock_wakes_at_relevant_boundaries_and_only_needed_collectors() {
    let s = settings(vec![scheduled("a", "09:30")]);
    let now = Local
        .from_local_datetime(&at("2026-11-06 09:00:00"))
        .single()
        .unwrap();
    assert_eq!(next_clock_wait(&s, now), Duration::from_secs(1800));
    let n = Needs::for_settings(&s);
    assert!(n.clock && !n.processes && !n.media && !n.foreground);
    assert_eq!(
        Needs::for_settings(&SceneSettings { paused: true, ..s }),
        Needs::default()
    );
}
