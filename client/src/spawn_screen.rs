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
use homage_shared::{hulls, sim};
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
    pub confirmed: bool,
    /// Detail-panel text (last clicked tile).
    pub detail: String,
}

impl Default for LoadoutState {
    fn default() -> Self {
        Self {
            hull: HullKind::Fighter,
            facility: None,
            confirmed: false,
            detail: String::new(),
        }
    }
}

/// Bank/points survive death server-side but the components die with the
/// ship; cache the last-seen values for dead-time display.
#[derive(Resource, Default)]
struct WealthCache {
    bank: u32,
    points: u32,
}

// UI marker components.
#[derive(Component)]
struct MapRoot;
#[derive(Component)]
struct MapArea;
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
struct ModuleTile(usize);
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

/// Placeholder module catalog (DESIGN §5) until fittings land in M3. Shown
/// dimmed; clicking explains. Costs are points.
struct ModEntry {
    name: &'static str,
    cost: u32,
    stocked: &'static str,
    blurb: &'static str,
}

const WEAPON_MODS: [ModEntry; 6] = [
    ModEntry { name: "Pulse Cannon", cost: 0, stocked: "everywhere", blurb: "Standard-issue autocannon. Reliable, unremarkable, yours." },
    ModEntry { name: "Scatter Gun", cost: 8, stocked: "any carrier", blurb: "Close-range spread. Ruins knife fights for the other guy." },
    ModEntry { name: "Long-Lance Railgun", cost: 20, stocked: "strike carrier", blurb: "Slow cycle, extreme velocity. Leading shots become sniping." },
    ModEntry { name: "Flak Burst", cost: 10, stocked: "any carrier", blurb: "Proximity detonation. The anti-fighter screen in a barrel." },
    ModEntry { name: "Torpedo", cost: 0, stocked: "any carrier", blurb: "Slow, huge, dumb-fire. Capital ships hate it." },
    ModEntry { name: "Mag-Torpedo", cost: 25, stocked: "strike carrier", blurb: "Mild tracking, lighter payload. Forgiveness in a tube." },
];

const ITEM_MODS: [ModEntry; 8] = [
    ModEntry { name: "Afterburner", cost: 10, stocked: "everywhere", blurb: "Heat-limited boost. Ride the redline." },
    ModEntry { name: "Blink Thruster", cost: 25, stocked: "outfitter", blurb: "Impulse dash on a cooldown. Be somewhere else." },
    ModEntry { name: "Shield Capacitor", cost: 25, stocked: "outfitter", blurb: "Timed damage absorb. An active block, not a health bar." },
    ModEntry { name: "Tractor Scoop", cost: 12, stocked: "res. controller", blurb: "Wider ore pickup that pulls fragments to you." },
    ModEntry { name: "Repair Drone", cost: 20, stocked: "outfitter", blurb: "Slow out-of-combat regeneration." },
    ModEntry { name: "Gyro Tuning", cost: 8, stocked: "everywhere", blurb: "+turn rate. The cheapest way to feel better." },
    ModEntry { name: "Armor Plate", cost: 8, stocked: "everywhere", blurb: "+HP, +mass. Trade dance for endurance." },
    ModEntry { name: "Compacted Hold", cost: 15, stocked: "res. controller", blurb: "+cargo capacity, worse handling. One more rock." },
];

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
        app.init_resource::<WealthCache>();
        app.add_systems(Startup, (setup_map_screen, setup_loadout_screen));
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
                    "click a highlighted facility to deploy there   ·   [L] loadout",
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
    area: Query<Entity, With<MapArea>>,
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
    if *ui != DeadUi::Map {
        return;
    }
    let now = time.elapsed_secs();
    if now - *last_refresh < 0.4 {
        return;
    }
    *last_refresh = now;
    let Ok(area) = area.single() else {
        return;
    };
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let mine = *own_team;

    for position in &asteroids {
        let marker = (
            MapMarker,
            marker_node(position.0, 4.0, Color::srgba(0.5, 0.48, 0.42, 0.8)),
        );
        commands.spawn(marker).insert(ChildOf(area));
    }
    for (entity, position, team, kind, color) in &ships {
        let friendly = mine == Some(*team);
        let clickable = friendly && *kind == HullKind::StrikeCarrier;
        let size = if clickable { 16.0 } else { 7.0 };
        let marker_color = if friendly || mine.is_none() {
            color.0
        } else {
            color.0.with_alpha(0.5)
        };
        let mut e = commands.spawn((MapMarker, marker_node(position.0, size, marker_color)));
        e.insert(ChildOf(area));
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
        let mut e = commands.spawn((MapMarker, marker_node(position.0, 22.0, color)));
        e.insert(ChildOf(area));
        if friendly {
            e.insert((Button, MapFacilityButton(entity)));
        }
    }
}

fn map_facility_clicks(
    mut ui: ResMut<DeadUi>,
    mut state: ResMut<LoadoutState>,
    buttons: Query<(&Interaction, &MapFacilityButton), Changed<Interaction>>,
    mut sender: Query<&mut MessageSender<SpawnOrder>, With<Client>>,
) {
    for (interaction, facility) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        state.facility = Some(facility.0);
        if let Ok(mut sender) = sender.single_mut() {
            sender.send::<OrdersChannel>(SpawnOrder {
                hull: state.hull,
                spawn_at: state.facility,
            });
        }
        *ui = DeadUi::Loadout;
    }
}

// ------------------------------------------------------------ loadout screen

fn module_grid(parent: &mut ChildSpawnerCommands, title: &str, mods: &'static [ModEntry], base: usize) {
    parent.spawn(text(title, 18.0, AMBER));
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|grid| {
            for (i, entry) in mods.iter().enumerate() {
                grid.spawn((
                    ModuleTile(base + i),
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
                    tile.spawn(text(entry.name, 13.0, DIM));
                    let cost = if entry.cost == 0 {
                        "free".to_string()
                    } else {
                        format!("{} pts", entry.cost)
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
                        module_grid(left, "WEAPONS", &WEAPON_MODS, 0);
                        module_grid(left, "ITEMS", &ITEM_MODS, WEAPON_MODS.len());
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
    ship: Query<(Option<&Bank>, Option<&Points>), (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut cache: ResMut<WealthCache>,
) {
    if let Ok((bank, points)) = ship.single() {
        cache.bank = bank.map_or(cache.bank, |b| b.0);
        cache.points = points.map_or(cache.points, |p| p.0);
    }
}

/// Death opens the map (map-first: the facility is the shop); respawn hides
/// everything and resets the per-death state.
fn dead_ui_lifecycle(
    alive: Query<(), (With<Predicted>, With<InputMarker<Inputs>>)>,
    mut ui: ResMut<DeadUi>,
    mut state: ResMut<LoadoutState>,
    mut died_at: Local<Option<f32>>,
    time: Res<Time>,
    mut map_root: Query<&mut Visibility, (With<MapRoot>, Without<LoadoutRoot>)>,
    mut loadout_root: Query<&mut Visibility, (With<LoadoutRoot>, Without<MapRoot>)>,
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
    if let Ok(mut visibility) = map_root.single_mut() {
        *visibility = if *ui == DeadUi::Map {
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
}

fn hull_tile_clicks(
    mut state: ResMut<LoadoutState>,
    mut tiles: Query<(&Interaction, &HullTile, &mut BackgroundColor), Changed<Interaction>>,
    mut sender: Query<&mut MessageSender<SpawnOrder>, With<Client>>,
) {
    for (interaction, tile, _) in &mut tiles {
        if *interaction != Interaction::Pressed {
            continue;
        }
        state.hull = tile.0;
        let stats = hulls::stats(tile.0);
        state.detail = format!(
            "{} — {}\n{}",
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
            });
        }
    }
}

fn hull_requirement_note(kind: HullKind) -> &'static str {
    match hulls::class(kind) {
        hulls::HullClass::Economy => "Spawns at the mothership or any friendly carrier.",
        hulls::HullClass::Combat => "Requires a live friendly strike carrier.",
        hulls::HullClass::CarrierType => "Built at the mothership only.",
    }
}

fn module_tile_clicks(
    mut state: ResMut<LoadoutState>,
    tiles: Query<(&Interaction, &ModuleTile), Changed<Interaction>>,
) {
    for (interaction, tile) in &tiles {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let entry = if tile.0 < WEAPON_MODS.len() {
            &WEAPON_MODS[tile.0]
        } else {
            &ITEM_MODS[tile.0 - WEAPON_MODS.len()]
        };
        state.detail = format!(
            "{} ({})\n{}\nStocked at: {}   —   fitting unlocks arrive in M3.",
            entry.name,
            if entry.cost == 0 { "free".into() } else { format!("{} pts", entry.cost) },
            entry.blurb,
            entry.stocked,
        );
    }
}

fn spawn_button_clicks(
    mut state: ResMut<LoadoutState>,
    buttons: Query<&Interaction, (With<SpawnButton>, Changed<Interaction>)>,
    mut orders: Query<&mut MessageSender<SpawnOrder>, With<Client>>,
    mut confirms: Query<&mut MessageSender<SpawnConfirm>, With<Client>>,
) {
    for interaction in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Ok(mut sender) = orders.single_mut() {
            sender.send::<OrdersChannel>(SpawnOrder {
                hull: state.hull,
                spawn_at: state.facility,
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
    mut hull_tiles: Query<(&HullTile, &mut BackgroundColor)>,
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
        t.0 = if ready {
            "SELECT SPAWN POINT — ready to deploy".to_string()
        } else {
            format!("SELECT SPAWN POINT — deployable in {ready_in:.1}s")
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
        t.0 = format!(
            "Weapon: {}\nUtility: — empty —\nHull mod: — empty —",
            if stats.weapon.is_some() { "hull default" } else { "none (unarmed)" },
        );
    }
    if let Ok(mut t) = texts.p4().single_mut() {
        t.0 = format!("{} ore   ·   {} pts", wealth.bank, wealth.points);
    }
    if let Ok(mut t) = texts.p5().single_mut() {
        let facility = match state.facility {
            None => "no spawn point — press [M]".to_string(),
            Some(entity) => match facility_lookup.get(entity) {
                Ok((Some(_), _)) => "Mothership".to_string(),
                Ok((_, Some(owner))) => {
                    let hull = names
                        .get(entity)
                        .map(|(_, kind)| hulls::display_name(*kind))
                        .unwrap_or("Carrier");
                    format!("{} [{}]", hull, owner.0.to_bits())
                }
                _ => "(facility lost — press [M])".to_string(),
            },
        };
        t.0 = format!("Spawning at: {facility}   [M] map");
    }
    if let Ok(mut t) = texts.p6().single_mut() {
        t.0 = if state.confirmed {
            if ready { "DEPLOYING…".into() } else { format!("DEPLOY IN {ready_in:.1}") }
        } else {
            "SPAWN".into()
        };
    }
    for (tile, mut bg) in &mut hull_tiles {
        bg.0 = if tile.0 == state.hull {
            TILE_SELECTED
        } else if hulls::stats(tile.0).cost > wealth.bank {
            TILE_DISABLED
        } else {
            TILE_BG
        };
    }
}
