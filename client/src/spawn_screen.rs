//! The spawn/docked UI (DESIGN §6, Savage XR model), two screens:
//!
//! 1. **Map screen** (first, on death): the whole battlefield; click a
//!    friendly facility to spawn from. The facility is the shop — it scopes
//!    which hulls it can field and (with M3 fittings) which modules it
//!    stocks.
//! 2. **Loadout screen**: weapons/items grids + detail panel on the left;
//!    hulls row, hull preview, equipped slots, and SPAWN on the right.
//!
//! There is no auto-respawn: SPAWN sends the confirm, and the server deploys
//! you once the respawn delay has elapsed. Later, docking at a facility
//! while alive opens this same UI (refit-in-place).

use avian2d::prelude::Position;
use bevy::prelude::*;
use homage_shared::protocol::*;
use homage_shared::{fittings, hulls, sim};
use crate::WealthCache;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::*;
use lightyear::prelude::*;

/// Which dead-time screen is up.
#[derive(Resource, Default, PartialEq, Clone, Copy, Debug)]
pub enum DeadUi {
    #[default]
    Hidden,
    Map,
    Loadout,
}

/// The player's in-progress spawn decision.
#[derive(Resource)]
pub struct LoadoutState {
    pub hull: HullKind,
    pub facility: Option<Entity>,
    pub loadout: Loadout,
    pub confirmed: bool,
    /// Detail-panel text (last clicked tile).
    pub detail: String,
}

impl Default for LoadoutState {
    fn default() -> Self {
        Self {
            hull: HullKind::Fighter,
            facility: None,
            loadout: Loadout::default(),
            confirmed: false,
            detail: String::new(),
        }
    }
}

// UI marker components.
#[derive(Component)]
struct MapRoot;
#[derive(Component)]
struct MapArea;
/// A panel that receives battlefield markers; the f32 scales marker sizes.
#[derive(Component)]
struct MarkerHost(f32);
#[derive(Component)]
struct MinimapRoot;
#[derive(Component)]
struct ScoreboardRoot;
#[derive(Component)]
struct ScoreboardText;
#[derive(Component)]
struct MatchBannerRoot;
#[derive(Component)]
struct MatchBannerText;
#[derive(Component)]
struct MapMarker;
#[derive(Component)]
struct MapFacilityButton(Entity);
#[derive(Component)]
struct MapStatusText;
#[derive(Component)]
struct LoadoutRoot;
#[derive(Component)]
struct HullTile(HullKind);
#[derive(Component)]
struct ModuleTile(FittingId);
#[derive(Component)]
struct DetailText;
#[derive(Component)]
struct EquippedText;
#[derive(Component)]
struct PreviewText;
#[derive(Component)]
struct CurrencyText;
#[derive(Component)]
struct FacilityContextText;
#[derive(Component)]
struct SpawnButton;
#[derive(Component)]
struct SpawnButtonText;

const PANEL_BG: Color = Color::srgba(0.04, 0.07, 0.11, 0.94);
const PANE_BG: Color = Color::srgba(0.09, 0.13, 0.19, 0.95);
const TILE_BG: Color = Color::srgba(0.14, 0.19, 0.27, 1.0);
const TILE_SELECTED: Color = Color::srgba(0.25, 0.42, 0.60, 1.0);
const TILE_DISABLED: Color = Color::srgba(0.10, 0.12, 0.15, 1.0);
const AMBER: Color = Color::srgb(1.0, 0.85, 0.3);
const DIM: Color = Color::srgb(0.55, 0.60, 0.68);
const BRIGHT: Color = Color::srgb(0.92, 0.95, 1.0);

pub struct SpawnScreenPlugin;

impl Plugin for SpawnScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeadUi>();
        app.init_resource::<LoadoutState>();
        app.add_systems(
            Startup,
            (
                setup_map_screen,
                setup_loadout_screen,
                setup_minimap,
                setup_scoreboard,
                setup_match_banner,
            ),
        );
        app.add_systems(
            Update,
            (
                cache_wealth,
                dead_ui_lifecycle,
                refresh_map_markers,
                map_facility_clicks,
                hull_tile_clicks,
                module_tile_clicks,
                spawn_button_clicks,
                screen_keys,
                scoreboard,
                match_banner,
                update_screen_texts,
            )
                .chain(),
        );
    }
}

fn text(value: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn pane(width: Val, height: Val) -> (Node, BackgroundColor) {
    (
        Node {
            width,
            height,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(8.0)),
            margin: UiRect::all(Val::Px(4.0)),
            overflow: Overflow::clip_y(),
            ..default()
        },
        BackgroundColor(PANE_BG),
    )
}

// ---------------------------------------------------------------- map screen

fn setup_map_screen(mut commands: Commands) {
    commands
        .spawn((
            MapRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            Visibility::Hidden,
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                MapStatusText,
                text("SELECT SPAWN POINT", 26.0, BRIGHT),
                Node {
                    margin: UiRect::bottom(Val::Px(10.0)),
                    ..default()
                },
            ));
            // The map: world (±MAP_HALF_*) mapped onto this panel by percent.
            root.spawn((
                MapArea,
                MarkerHost(1.0),
                Node {
                    width: Val::Percent(80.0),
                    // Keep the world's 3:2 aspect.
                    aspect_ratio: Some(sim::MAP_HALF_WIDTH / sim::MAP_HALF_HEIGHT),
                    max_height: Val::Percent(75.0),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ));
            root.spawn((
                text(
                    "click a highlighted facility to deploy there   |   [L] loadout",
                    16.0,
                    DIM,
                ),
                Node {
                    margin: UiRect::top(Val::Px(10.0)),
                    ..default()
                },
            ));
        });
}

/// Percent position of a world point on the map panel.
fn map_percent(world: Vec2) -> (f32, f32) {
    let x = (world.x + sim::MAP_HALF_WIDTH) / (2.0 * sim::MAP_HALF_WIDTH) * 100.0;
    let y = (1.0 - (world.y + sim::MAP_HALF_HEIGHT) / (2.0 * sim::MAP_HALF_HEIGHT)) * 100.0;
    (x.clamp(0.0, 99.0), y.clamp(0.0, 99.0))
}

fn marker_node(world: Vec2, size: f32, color: Color) -> (Node, BackgroundColor) {
    let (x, y) = map_percent(world);
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(x),
            top: Val::Percent(y),
            width: Val::Px(size),
            height: Val::Px(size),
            ..default()
        },
        BackgroundColor(color),
    )
}

/// Rebuild the map markers a few times a second while the map is up.
#[allow(clippy::too_many_arguments)]
fn refresh_map_markers(
    time: Res<Time>,
    mut last_refresh: Local<f32>,
    ui: Res<DeadUi>,
    mut commands: Commands,
    area: Query<(Entity, &MarkerHost)>,
    old: Query<Entity, With<MapMarker>>,
    own_team_source: Query<&Team, (With<PlayerId>, With<Predicted>)>,
    mut own_team: Local<Option<Team>>,
    motherships: Query<(Entity, &Position, &Team), With<Mothership>>,
    ships: Query<(Entity, &Position, &Team, &HullKind, &PlayerColor), With<PlayerId>>,
    asteroids: Query<&Position, With<Asteroid>>,
) {
    if let Ok(team) = own_team_source.single() {
        *own_team = Some(*team);
    }
    let now = time.elapsed_secs();
    if now - *last_refresh < 0.4 {
        return;
    }
    *last_refresh = now;
    let _ = *ui;
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let mine = *own_team;

    for (host, scale) in area.iter().map(|(e, h)| (e, h.0)) {
        for position in &asteroids {
            let marker = (
                MapMarker,
                marker_node(position.0, 4.0 * scale, Color::srgba(0.5, 0.48, 0.42, 0.8)),
            );
            commands.spawn(marker).insert(ChildOf(host));
        }
        for (entity, position, team, kind, color) in &ships {
            let friendly = mine == Some(*team);
            let clickable = friendly && *kind == HullKind::StrikeCarrier;
            let size = if clickable { 16.0 } else { 7.0 } * scale;
            let marker_color = if friendly || mine.is_none() {
                color.0
            } else {
                color.0.with_alpha(0.5)
            };
            let mut e =
                commands.spawn((MapMarker, marker_node(position.0, size, marker_color)));
            e.insert(ChildOf(host));
            if clickable {
                e.insert((Button, MapFacilityButton(entity)));
            }
        }
        for (entity, position, team) in &motherships {
            let friendly = mine == Some(*team);
            let color = match team {
                Team::Blue => Color::srgb(0.35, 0.55, 1.0),
                Team::Red => Color::srgb(1.0, 0.35, 0.35),
            };
            let color = if friendly { color } else { color.with_alpha(0.5) };
            let mut e = commands.spawn((MapMarker, marker_node(position.0, 22.0 * scale, color)));
            e.insert(ChildOf(host));
            if friendly {
                e.insert((Button, MapFacilityButton(entity)));
            }
        }
    }
}

fn map_facility_clicks(
    mut ui: ResMut<DeadUi>,

    mut state: ResMut<LoadoutState>,
    wealth: Res<WealthCache>,
    facility_lookup: Query<(Option<&Mothership>, Option<&HullKind>)>,
    buttons: Query<(&Interaction, &MapFacilityButton), Changed<Interaction>>,
    mut sender: Query<&mut MessageSender<SpawnOrder>, With<Client>>,
) {
    for (interaction, facility) in &buttons {
        if *interaction != Interaction::Pressed || *ui != DeadUi::Map {
            continue;
        }
        state.facility = Some(facility.0);
        // The facility scopes the shop: if the standing hull can't deploy
        // here, reset to the always-valid fighter and say why.
        let kind = facility_kind(state.facility, &facility_lookup);
        if let Err(reason) = hull_gate(state.hull, kind, wealth.bank) {
            state.detail = format!(
                "Selection reset to Fighter: {} cannot deploy here.\n({reason})",
                hulls::display_name(state.hull),
            );
            state.hull = HullKind::Fighter;
        }
        if let Ok(mut sender) = sender.single_mut() {
            sender.send::<OrdersChannel>(SpawnOrder {
                hull: state.hull,
                spawn_at: state.facility,
                loadout: state.loadout,
            });
        }
        *ui = DeadUi::Loadout;
    }
}

/// Corner minimap, visible while flying (M3.5).
fn setup_minimap(mut commands: Commands) {
    commands.spawn((
        MinimapRoot,
        MarkerHost(0.45),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(10.0),
            bottom: Val::Px(10.0),
            width: Val::Px(240.0),
            aspect_ratio: Some(sim::MAP_HALF_WIDTH / sim::MAP_HALF_HEIGHT),
            ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.07, 0.11, 0.72)),
        Visibility::Hidden,
        GlobalZIndex(5),
    ));
}

/// Hold-Tab scoreboard (M3.5): both rosters with K/D and points.
fn setup_scoreboard(mut commands: Commands) {
    commands
        .spawn((
            ScoreboardRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    padding: UiRect::all(Val::Px(18.0)),
                    min_width: Val::Px(520.0),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|panel| {
                panel.spawn((ScoreboardText, text("", 16.0, BRIGHT)));
            });
        });
}

/// Big center banner while a match result is fresh (the ~10s intermission).
fn setup_match_banner(mut commands: Commands) {
    commands
        .spawn((
            MatchBannerRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                top: Val::Percent(18.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(30),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(30.0), Val::Px(14.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|panel| {
                panel.spawn((MatchBannerText, text("", 34.0, AMBER)));
            });
        });
}

fn match_banner(
    time: Res<Time>,
    result: Res<crate::LastMatchResult>,
    mut root: Query<&mut Visibility, With<MatchBannerRoot>>,
    mut banner: Query<&mut Text, With<MatchBannerText>>,
) {
    let Ok(mut visibility) = root.single_mut() else {
        return;
    };
    let show = result
        .0
        .map(|(_, at)| time.elapsed_secs() - at < sim::MATCH_RESET_TICKS as f32 * sim::TICK_DT)
        .unwrap_or(false);
    *visibility = if show {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if show {
        if let (Ok(mut text), Some((winner, _))) = (banner.single_mut(), result.0) {
            text.0 = format!("{winner:?} TEAM WINS - new match starting...");
        }
    }
}

/// Rebuild the scoreboard text and toggle its visibility while Tab is held.
fn scoreboard(
    keys: Res<ButtonInput<KeyCode>>,
    mut root: Query<&mut Visibility, With<ScoreboardRoot>>,
    mut board: Query<&mut Text, With<ScoreboardText>>,
    roster: Query<(&RosterEntry, &Team, &Kills, &Deaths, &Points)>,
) {
    let Ok(mut visibility) = root.single_mut() else {
        return;
    };
    if !keys.pressed(KeyCode::Tab) {
        *visibility = Visibility::Hidden;
        return;
    }
    *visibility = Visibility::Visible;
    let Ok(mut text) = board.single_mut() else {
        return;
    };
    let mut lines = String::new();
    for team in [Team::Blue, Team::Red] {
        lines.push_str(&format!("=== {team:?} ===
"));
        lines.push_str(&format!("{:<12} {:>5} {:>5} {:>7}
", "player", "K", "D", "pts"));
        let mut entries: Vec<_> = roster
            .iter()
            .filter(|(_, t, ..)| **t == team)
            .map(|(entry, _, kills, deaths, points)| {
                (entry.0.to_bits(), kills.0, deaths.0, points.0)
            })
            .collect();
        entries.sort_by_key(|(_, _, _, points)| std::cmp::Reverse(*points));
        for (id, kills, deaths, points) in entries {
            lines.push_str(&format!("P{id:<11} {kills:>5} {deaths:>5} {points:>7}
"));
        }
        lines.push('\n');
    }
    text.0 = lines;
}

// ------------------------------------------------------------ loadout screen

fn module_grid(parent: &mut ChildSpawnerCommands, title: &str, slots: &[fittings::Slot]) {
    parent.spawn(text(title, 18.0, AMBER));
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|grid| {
            for def in fittings::CATALOG.iter().filter(|d| slots.contains(&d.slot)) {
                grid.spawn((
                    ModuleTile(def.id),
                    Button,
                    Node {
                        width: Val::Percent(23.0),
                        margin: UiRect::all(Val::Px(3.0)),
                        padding: UiRect::all(Val::Px(6.0)),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    BackgroundColor(TILE_DISABLED),
                ))
                .with_children(|tile| {
                    tile.spawn(text(def.name, 13.0, BRIGHT));
                    let cost = if def.cost == 0 {
                        "free".to_string()
                    } else {
                        format!("{} pts", def.cost)
                    };
                    tile.spawn(text(&cost, 12.0, DIM));
                });
            }
        });
}

fn setup_loadout_screen(mut commands: Commands) {
    commands
        .spawn((
            LoadoutRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            Visibility::Hidden,
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Percent(88.0),
                    height: Val::Percent(86.0),
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|panel| {
                // LEFT page: weapons grid, items grid, detail panel.
                panel
                    .spawn(pane(Val::Percent(50.0), Val::Percent(100.0)))
                    .with_children(|left| {
                        module_grid(left, "WEAPONS", &[fittings::Slot::Weapon]);
                        module_grid(
                            left,
                            "ITEMS",
                            &[fittings::Slot::Utility, fittings::Slot::HullMod],
                        );
                        left.spawn((
                            Node {
                                flex_grow: 1.0,
                                margin: UiRect::top(Val::Px(6.0)),
                                padding: UiRect::all(Val::Px(8.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                        ))
                        .with_children(|detail| {
                            detail.spawn((DetailText, text("", 14.0, BRIGHT)));
                        });
                    });
                // RIGHT page: hulls row, preview, equipped, spawn.
                panel
                    .spawn(pane(Val::Percent(50.0), Val::Percent(100.0)))
                    .with_children(|right| {
                        right
                            .spawn(Node {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn(text("HULLS", 18.0, AMBER));
                                row.spawn((CurrencyText, text("", 16.0, AMBER)));
                            });
                        right
                            .spawn(Node {
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                ..default()
                            })
                            .with_children(|row| {
                                for kind in hulls::PURCHASABLE {
                                    row.spawn((
                                        HullTile(kind),
                                        Button,
                                        Node {
                                            width: Val::Percent(18.0),
                                            margin: UiRect::all(Val::Px(3.0)),
                                            padding: UiRect::all(Val::Px(6.0)),
                                            flex_direction: FlexDirection::Column,
                                            ..default()
                                        },
                                        BackgroundColor(TILE_BG),
                                    ))
                                    .with_children(|tile| {
                                        tile.spawn(text(hulls::display_name(kind), 13.0, BRIGHT));
                                        let stats = hulls::stats(kind);
                                        let cost = if stats.cost == 0 {
                                            "free".to_string()
                                        } else {
                                            format!("{} ore", stats.cost)
                                        };
                                        tile.spawn(text(&cost, 12.0, DIM));
                                    });
                                }
                            });
                        right.spawn((
                            PreviewText,
                            text("", 16.0, BRIGHT),
                            Node {
                                flex_grow: 1.0,
                                margin: UiRect::vertical(Val::Px(8.0)),
                                ..default()
                            },
                        ));
                        right.spawn(text("EQUIPPED", 18.0, AMBER));
                        right.spawn((EquippedText, text("", 14.0, BRIGHT)));
                        right
                            .spawn(Node {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                margin: UiRect::top(Val::Px(8.0)),
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((FacilityContextText, text("", 14.0, DIM)));
                                row.spawn((
                                    SpawnButton,
                                    Button,
                                    Node {
                                        padding: UiRect::axes(Val::Px(26.0), Val::Px(10.0)),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.16, 0.45, 0.20)),
                                ))
                                .with_children(|b| {
                                    b.spawn((SpawnButtonText, text("SPAWN", 20.0, BRIGHT)));
                                });
                            });
                    });
            });
        });
}

// ------------------------------------------------------------------ behavior

/// Cache wealth while alive; the ship's components vanish on death.
fn cache_wealth(
    ship: Query<
        (Option<&Bank>, Option<&Points>, Option<&UnlockedFittings>),
        (With<Predicted>, With<InputMarker<Inputs>>),
    >,
    mut cache: ResMut<WealthCache>,
) {
    if let Ok((bank, points, unlocked)) = ship.single() {
        cache.bank = bank.map_or(cache.bank, |b| b.0);
        cache.points = points.map_or(cache.points, |p| p.0);
        if let Some(unlocked) = unlocked {
            cache.unlocked = unlocked.0.iter().copied().collect();
        }
    }
}

/// Death opens the map (map-first: the facility is the shop); respawn hides
/// everything and resets the per-death state.
#[allow(clippy::type_complexity)]
fn dead_ui_lifecycle(
    alive: Query<(), (With<Predicted>, With<InputMarker<Inputs>>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<DeadUi>,
    mut state: ResMut<LoadoutState>,
    mut died_at: Local<Option<f32>>,
    time: Res<Time>,
    mut map_root: Query<
        &mut Visibility,
        (With<MapRoot>, Without<LoadoutRoot>, Without<MinimapRoot>),
    >,
    mut loadout_root: Query<
        &mut Visibility,
        (With<LoadoutRoot>, Without<MapRoot>, Without<MinimapRoot>),
    >,
    mut minimap_root: Query<
        &mut Visibility,
        (With<MinimapRoot>, Without<MapRoot>, Without<LoadoutRoot>),
    >,
) {
    let alive = !alive.is_empty();
    if alive {
        *ui = DeadUi::Hidden;
        *died_at = None;
        state.confirmed = false;
    } else if *ui == DeadUi::Hidden {
        *ui = DeadUi::Map;
        state.facility = None;
        died_at.get_or_insert(time.elapsed_secs());
    }
    // Hold M while flying for the full battlefield map (view-only).
    let alive_map = alive && keys.pressed(KeyCode::KeyM);
    if let Ok(mut visibility) = map_root.single_mut() {
        *visibility = if *ui == DeadUi::Map || alive_map {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut visibility) = loadout_root.single_mut() {
        *visibility = if *ui == DeadUi::Loadout {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut visibility) = minimap_root.single_mut() {
        *visibility = if alive && !alive_map {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn hull_tile_clicks(
    mut state: ResMut<LoadoutState>,
    wealth: Res<WealthCache>,
    facility_lookup: Query<(Option<&Mothership>, Option<&HullKind>)>,
    mut tiles: Query<(&Interaction, &HullTile), Changed<Interaction>>,
    mut sender: Query<&mut MessageSender<SpawnOrder>, With<Client>>,
) {
    for (interaction, tile) in &mut tiles {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let facility = facility_kind(state.facility, &facility_lookup);
        if let Err(reason) = hull_gate(tile.0, facility, wealth.bank) {
            state.detail = format!("{}: {reason}", hulls::display_name(tile.0));
            continue;
        }
        state.hull = tile.0;
        let stats = hulls::stats(tile.0);
        state.detail = format!(
            "{}: {}\n{}",
            hulls::display_name(tile.0),
            match stats.archetype {
                hulls::Archetype::Pilot => "Pilot: you are the weapon.",
                hulls::Archetype::Gunship => "Gunship: fly the platform, the mouse aims the turret.",
                hulls::Archetype::Captain => "Captain: omnidirectional drift; presence is power.",
            },
            hull_requirement_note(tile.0),
        );
        if let Ok(mut sender) = sender.single_mut() {
            sender.send::<OrdersChannel>(SpawnOrder {
                hull: state.hull,
                spawn_at: state.facility,
                loadout: state.loadout,
            });
        }
    }
}

/// What kind of facility the player has selected on the map.
#[derive(Clone, Copy, PartialEq)]
enum FacilityKind {
    Mothership,
    StrikeCarrier,
}

fn facility_kind(
    facility: Option<Entity>,
    lookup: &Query<(Option<&Mothership>, Option<&HullKind>)>,
) -> Option<FacilityKind> {
    match lookup.get(facility?) {
        Ok((Some(_), _)) => Some(FacilityKind::Mothership),
        Ok((_, Some(HullKind::StrikeCarrier))) => Some(FacilityKind::StrikeCarrier),
        _ => None,
    }
}

/// Can this hull be bought and fielded right now? Err carries the reason the
/// player sees. The server enforces the same rules; this is the honest UI.
fn hull_gate(
    kind: HullKind,
    facility: Option<FacilityKind>,
    bank: u32,
) -> Result<(), String> {
    let Some(facility) = facility else {
        return Err("Choose a spawn point on the map first - press [M].".into());
    };
    let class_ok = match hulls::class(kind) {
        hulls::HullClass::Economy => true,
        hulls::HullClass::Combat => facility == FacilityKind::StrikeCarrier,
        hulls::HullClass::CarrierType => facility == FacilityKind::Mothership,
    };
    if !class_ok {
        return Err(match hulls::class(kind) {
            hulls::HullClass::Combat => {
                "Combat hulls deploy from a strike carrier - pick one on the map [M].".into()
            }
            _ => "Carrier-type hulls are built at the mothership - pick it on the map [M].".into(),
        });
    }
    let cost = hulls::stats(kind).cost;
    if cost > bank {
        return Err(format!("Not enough ore: costs {cost}, you have {bank}."));
    }
    Ok(())
}

fn hull_requirement_note(kind: HullKind) -> &'static str {
    match hulls::class(kind) {
        hulls::HullClass::Economy => "Spawns at the mothership or any friendly carrier.",
        hulls::HullClass::Combat => "Requires a live friendly strike carrier.",
        hulls::HullClass::CarrierType => "Built at the mothership only.",
    }
}

/// Unlock-or-equip: locked tiles buy the unlock with points; unlocked tiles
/// equip (or unequip, for optional slots) when the facility stocks them.
fn module_tile_clicks(
    mut state: ResMut<LoadoutState>,
    wealth: Res<WealthCache>,
    facility_lookup: Query<(Option<&Mothership>, Option<&HullKind>)>,
    tiles: Query<(&Interaction, &ModuleTile), Changed<Interaction>>,
    mut orders: Query<&mut MessageSender<UnlockOrder>, With<Client>>,
    mut spawn_orders: Query<&mut MessageSender<SpawnOrder>, With<Client>>,
) {
    for (interaction, tile) in &tiles {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let def = fittings::def(tile.0);
        let stocked_note = match def.stocking {
            fittings::Stocking::Everywhere => "stocked everywhere",
            fittings::Stocking::AnyCarrier => "stocked at carriers",
            fittings::Stocking::StrikeCarrierOnly => "stocked at strike carriers",
        };
        // Not yet unlocked: this click is a purchase attempt.
        if !wealth.has_unlock(tile.0) {
            if wealth.points < def.cost {
                state.detail = format!(
                    "{}: {}\n{stocked_note}\nUnlock costs {} pts; you have {}.",
                    def.name, def.blurb, def.cost, wealth.points
                );
            } else {
                if let Ok(mut sender) = orders.single_mut() {
                    sender.send::<OrdersChannel>(UnlockOrder { fitting: tile.0 });
                }
                state.detail = format!(
                    "{}: unlocking for {} pts (yours for the match). Click again to equip.",
                    def.name, def.cost
                );
            }
            continue;
        }
        // Unlocked: equip if the selected facility stocks it.
        let stocked_here = facility_kind(state.facility, &facility_lookup)
            .map(|f| match f {
                FacilityKind::Mothership => fittings::stocked_at(
                    def.stocking,
                    fittings::SpawnFacility::Mothership,
                ),
                FacilityKind::StrikeCarrier => fittings::stocked_at(
                    def.stocking,
                    fittings::SpawnFacility::StrikeCarrier,
                ),
            })
            .unwrap_or(false);
        if !stocked_here {
            state.detail = format!(
                "{}: {}\nNot stocked at this facility ({stocked_note}). Pick another spawn point [M].",
                def.name, def.blurb
            );
            continue;
        }
        match def.slot {
            fittings::Slot::Weapon => state.loadout.weapon = tile.0,
            fittings::Slot::Utility => {
                state.loadout.utility =
                    (state.loadout.utility != Some(tile.0)).then_some(tile.0);
            }
            fittings::Slot::HullMod => {
                state.loadout.hull_mod =
                    (state.loadout.hull_mod != Some(tile.0)).then_some(tile.0);
            }
        }
        state.detail = format!("{} equipped.\n{}", def.name, def.blurb);
        if let Ok(mut sender) = spawn_orders.single_mut() {
            sender.send::<OrdersChannel>(SpawnOrder {
                hull: state.hull,
                spawn_at: state.facility,
                loadout: state.loadout,
            });
        }
    }
}

fn spawn_button_clicks(
    mut state: ResMut<LoadoutState>,
    wealth: Res<WealthCache>,
    facility_lookup: Query<(Option<&Mothership>, Option<&HullKind>)>,
    buttons: Query<&Interaction, (With<SpawnButton>, Changed<Interaction>)>,
    mut orders: Query<&mut MessageSender<SpawnOrder>, With<Client>>,
    mut confirms: Query<&mut MessageSender<SpawnConfirm>, With<Client>>,
) {
    for interaction in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let facility = facility_kind(state.facility, &facility_lookup);
        if let Err(reason) = hull_gate(state.hull, facility, wealth.bank) {
            state.detail = format!("Can't deploy: {reason}");
            continue;
        }
        if let Ok(mut sender) = orders.single_mut() {
            sender.send::<OrdersChannel>(SpawnOrder {
                hull: state.hull,
                spawn_at: state.facility,
                loadout: state.loadout,
            });
        }
        if let Ok(mut sender) = confirms.single_mut() {
            sender.send::<OrdersChannel>(SpawnConfirm);
        }
        state.confirmed = true;
    }
}

fn screen_keys(keys: Res<ButtonInput<KeyCode>>, mut ui: ResMut<DeadUi>) {
    match *ui {
        DeadUi::Map => {
            if keys.just_pressed(KeyCode::KeyL) {
                *ui = DeadUi::Loadout;
            }
        }
        DeadUi::Loadout => {
            if keys.just_pressed(KeyCode::KeyM) || keys.just_pressed(KeyCode::Escape) {
                *ui = DeadUi::Map;
            }
        }
        DeadUi::Hidden => {}
    }
}

/// Keep every text element and tile highlight current.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_screen_texts(
    ui: Res<DeadUi>,
    state: Res<LoadoutState>,
    wealth: Res<WealthCache>,
    time: Res<Time>,
    mut died_at: Local<Option<f32>>,
    alive: Query<(), (With<Predicted>, With<InputMarker<Inputs>>)>,
    names: Query<(&PlayerId, &HullKind), Without<Mothership>>,
    facility_lookup: Query<(Option<&Mothership>, Option<&PlayerId>)>,
    mut texts: ParamSet<(
        Query<&mut Text, With<MapStatusText>>,
        Query<&mut Text, With<DetailText>>,
        Query<&mut Text, With<PreviewText>>,
        Query<&mut Text, With<EquippedText>>,
        Query<&mut Text, With<CurrencyText>>,
        Query<&mut Text, With<FacilityContextText>>,
        Query<&mut Text, With<SpawnButtonText>>,
    )>,
    gate_lookup: Query<(Option<&Mothership>, Option<&HullKind>)>,
    mut hull_tiles: Query<(&HullTile, &mut BackgroundColor), Without<ModuleTile>>,
    mut module_tiles: Query<(&ModuleTile, &mut BackgroundColor), Without<HullTile>>,
) {
    if *ui == DeadUi::Hidden {
        *died_at = None;
        return;
    }
    if alive.is_empty() {
        died_at.get_or_insert(time.elapsed_secs());
    }
    let ready_in = died_at.map_or(0.0, |died| {
        (sim::RESPAWN_DELAY_TICKS as f32 * sim::TICK_DT - (time.elapsed_secs() - died)).max(0.0)
    });
    let ready = ready_in <= 0.05;

    if let Ok(mut t) = texts.p0().single_mut() {
        t.0 = if !alive.is_empty() {
            "BATTLEFIELD".to_string()
        } else if ready {
            "SELECT SPAWN POINT - ready to deploy".to_string()
        } else {
            format!("SELECT SPAWN POINT - deployable in {ready_in:.1}s")
        };
    }
    if let Ok(mut t) = texts.p1().single_mut() {
        t.0 = state.detail.clone();
    }
    let stats = hulls::stats(state.hull);
    if let Ok(mut t) = texts.p2().single_mut() {
        t.0 = format!(
            "{}\n\narchetype: {:?}\nhull: {} hp\nmax speed: {:.0}\ncargo hold: {}\ncost: {}",
            hulls::display_name(state.hull).to_uppercase(),
            stats.archetype,
            stats.health,
            stats.max_speed,
            stats.cargo_capacity,
            if stats.cost == 0 { "free".to_string() } else { format!("{} ore", stats.cost) },
        );
    }
    if let Ok(mut t) = texts.p3().single_mut() {
        let weapon = if stats.weapon.is_some() {
            fittings::def(state.loadout.weapon).name
        } else {
            "none (hull is unarmed)"
        };
        let slot = |f: Option<FittingId>| {
            f.map(|f| fittings::def(f).name).unwrap_or("(empty)")
        };
        t.0 = format!(
            "Weapon: {weapon}\nUtility: {}\nHull mod: {}",
            slot(state.loadout.utility),
            slot(state.loadout.hull_mod),
        );
    }
    if let Ok(mut t) = texts.p4().single_mut() {
        t.0 = format!("{} ore   |   {} pts", wealth.bank, wealth.points);
    }
    if let Ok(mut t) = texts.p5().single_mut() {
        let facility = match state.facility {
            None => "no spawn point - press [M]".to_string(),
            Some(entity) => match facility_lookup.get(entity) {
                Ok((Some(_), _)) => "Mothership".to_string(),
                Ok((_, Some(owner))) => {
                    let hull = names
                        .get(entity)
                        .map(|(_, kind)| hulls::display_name(*kind))
                        .unwrap_or("Carrier");
                    format!("{} [{}]", hull, owner.0.to_bits())
                }
                _ => "(facility lost - press [M])".to_string(),
            },
        };
        t.0 = format!("Spawning at: {facility}   [M] map");
    }
    if let Ok(mut t) = texts.p6().single_mut() {
        t.0 = if state.confirmed {
            if ready { "DEPLOYING...".into() } else { format!("DEPLOY IN {ready_in:.1}") }
        } else {
            "SPAWN".into()
        };
    }
    let facility = facility_kind(state.facility, &gate_lookup);
    let spawn_facility = facility.map(|f| match f {
        FacilityKind::Mothership => fittings::SpawnFacility::Mothership,
        FacilityKind::StrikeCarrier => fittings::SpawnFacility::StrikeCarrier,
    });
    for (tile, mut bg) in &mut module_tiles {
        let def = fittings::def(tile.0);
        let equipped = state.loadout.weapon == tile.0
            || state.loadout.utility == Some(tile.0)
            || state.loadout.hull_mod == Some(tile.0);
        let stocked = spawn_facility
            .map(|f| fittings::stocked_at(def.stocking, f))
            .unwrap_or(false);
        bg.0 = if equipped {
            TILE_SELECTED
        } else if wealth.has_unlock(tile.0) && stocked {
            TILE_BG
        } else {
            TILE_DISABLED
        };
    }
    for (tile, mut bg) in &mut hull_tiles {
        bg.0 = if tile.0 == state.hull {
            TILE_SELECTED
        } else if hull_gate(tile.0, facility, wealth.bank).is_err() {
            TILE_DISABLED
        } else {
            TILE_BG
        };
    }
}
