use core::str::FromStr;
use crossterm::{
    event::{self, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use hifitime::prelude::*;

use ratatui::{
    prelude::{CrosstermBackend, Terminal},
    style::{Color, Stylize},
    widgets::{
        canvas::{Canvas, Map, MapResolution},
        Block, Borders,
    },
};
use sgp4::{Elements, Prediction};
use std::{f64::consts::PI, io::stdout};

/// Based on https://github.com/colej4/satapp/blob/main/src-tauri/src/tracking.rs#L419-L423
struct SphericalPoint {
    rho: f64,
    theta: f64,
    phi: f64,
}

/// Based on https://github.com/colej4/satapp/blob/be4a3831134475396bab3639b8add1b337e5b93c/src-tauri/src/tracking.rs#L425-L429
pub struct RectangularPoint {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Based on https://github.com/colej4/satapp/blob/be4a3831134475396bab3639b8add1b337e5b93c/src-tauri/src/tracking.rs#L431-L434
pub struct GroundPos {
    pub lat: f64,
    pub lon: f64,
}

/// takes in a point in rectangular coordinates, returns spherical coordinates
/// Based on https://github.com/colej4/satapp/blob/be4a3831134475396bab3639b8add1b337e5b93c/src-tauri/src/tracking.rs#L11-L21
fn rect_to_spherical(r: &RectangularPoint) -> SphericalPoint {
    let rho = f64::sqrt(r.x.powi(2) + r.y.powi(2) + r.z.powi(2));
    let theta = f64::atan2(r.y, r.x);
    let phi = f64::atan2(f64::sqrt(r.x.powf(2.0) + r.y.powf(2.0)), r.z);
    return SphericalPoint {
        rho: rho,
        theta: theta,
        phi: phi,
    };
}

/// Based on https://github.com/colej4/satapp/blob/be4a3831134475396bab3639b8add1b337e5b93c/src-tauri/src/tracking.rs#L30-L42
fn spherical_to_lat_lon(s: &SphericalPoint, time: Epoch) -> GroundPos {
    let lat = ((s.phi * 180.0 / PI) - 90.0) * -1.0;
    let sidereal_time = calc_gmst(time) as f64 / 86400.0 * 360.0;
    let mut lon = ((s.theta * 180.0 / PI) - sidereal_time) % 360.0;
    if lon < -180.0 {
        lon += 360.0;
    }
    if lon > 180.0 {
        lon -= 360.0;
    }

    return GroundPos { lat: lat, lon: lon };
}

/// returns current gmst in seconds
/// Based on https://github.com/colej4/satapp/blob/be4a3831134475396bab3639b8add1b337e5b93c/src-tauri/src/tracking.rs#L44-L53
pub fn calc_gmst(time: Epoch) -> f64 {
    let now = time;
    let s = (now.to_et_seconds() % 86400.0) - 43269.1839244;
    let t = (now.to_jde_et_days() - s / 86400.0 - 2451545.0) / 36525.0; //days since january 1, 4713 BC noon
    let h0 = 24110.54841 + 8640184.812866 * t + 0.093104 * t.powi(2); //the sidereal time at midnight this morning
    let h1 = 1.00273790935 + 5.9 * 10.0f64.powf(-11.0) * t;
    let rot = (h0 + h1 * s) % 86400.0;
    return rot;
}

/// Based on https://github.com/colej4/satapp/blob/be4a3831134475396bab3639b8add1b337e5b93c/src-tauri/src/tracking.rs#L60-L77
fn get_prediction(time: Epoch, elements: &Elements) -> Option<Prediction> {
    let epoch = Epoch::from_str(format!("{} UTC", elements.datetime).as_str()).unwrap();
    let duration = time - epoch;
    let constants = sgp4::Constants::from_elements(&elements).unwrap();
    //println!("last epoch was at {}", epoch);
    //println!("last epoch was {} ago", duration);
    let prediction =
        constants.propagate(sgp4::MinutesSinceEpoch(duration.to_seconds() / 60 as f64));
    match prediction {
        Ok(pred) => return Some(pred),
        Err(_) => {
            //println!("{:?} at sat {}", e, elements.norad_id);
            return None;
        }
    }

    //println!("        r = {:?} km", prediction.position);
    //println!("        ṙ = {:?} km.s⁻¹", prediction.velocity);
}

/// Based on https://github.com/colej4/satapp/blob/be4a3831134475396bab3639b8add1b337e5b93c/src-tauri/src/tracking.rs#L79-L94
pub fn get_sat_lat_lon(time: Epoch, elements: &Elements) -> Option<GroundPos> {
    let pred_option = get_prediction(time, elements);
    if let Some(prediction) = pred_option {
        let x = prediction.position[0];
        let y = prediction.position[1];
        let z = prediction.position[2];
        let rect = RectangularPoint { x: x, y: y, z: z };
        let spher = rect_to_spherical(&rect);
        let g = spherical_to_lat_lon(&spher, time);
        //println!("sat is at ({}, {}) at {:?}", g.lat, g.lon, time);
        return Some(g);
    } else {
        return None;
    }
}

fn main() -> anyhow::Result<()> {
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    terminal
        .draw(|frame| {
            let area = frame.size();
            frame.render_widget(
                Canvas::default()
                    .block(
                        Block::default()
                            .title("Fetching orbits from celestrak...")
                            .borders(Borders::ALL),
                    )
                    .x_bounds([-180.0, 180.0])
                    .y_bounds([-90.0, 90.0])
                    .paint(|ctx| {
                        ctx.draw(&Map {
                            resolution: MapResolution::High,
                            color: Color::White,
                        });
                        ctx.layer();
                    }),
                area,
            )
        })
        .unwrap();

    let response = ureq::get("https://celestrak.com/NORAD/elements/supplemental/sup-gp.php")
        .query("NAME", "KUIPER")
        .query("FORMAT", "json")
        .call()?;
    let elements_vec: Vec<sgp4::Elements> = response.into_json()?;
    let kuiper_sats = elements_vec
        .iter()
        .filter(|entry| {
            entry
                .object_name
                .as_ref()
                .is_some_and(|name| name.starts_with("KUIPER"))
        })
        .collect::<Vec<&Elements>>();
    loop {
        let current_time = Epoch::now().unwrap();
        let next_orbit_end = current_time + (Unit::Minute * 94.5);
        let predictions = TimeSeries::exclusive(current_time, next_orbit_end, Unit::Minute * 2.5);

        let sat_pos: Vec<(&&Elements, Vec<GroundPos>)> = kuiper_sats
            .iter()
            .map(|sat| {
                (
                    sat,
                    predictions
                        .clone()
                        .map(|time| get_sat_lat_lon(time, sat).unwrap())
                        .collect(),
                )
            })
            .collect();
        terminal.draw(|frame| {
            let area = frame.size();
            frame.render_widget(
                Canvas::default()
                    .block(
                        Block::default()
                            .title(current_time.to_string())
                            .borders(Borders::ALL),
                    )
                    .x_bounds([-180.0, 180.0])
                    .y_bounds([-90.0, 90.0])
                    .paint(|ctx| {
                        ctx.draw(&Map {
                            resolution: MapResolution::High,
                            color: Color::White,
                        });
                        ctx.layer();
                        sat_pos.iter().for_each(|(sat, pos)| {
                            pos.iter().for_each(|prediction| {
                                ctx.print(prediction.lon, prediction.lat, ".".red())
                            });
                            ctx.print(
                                pos[0].lon,
                                pos[0].lat,
                                format!(
                                    "🛰️{}",
                                    sat.object_name
                                        .as_ref()
                                        .unwrap()
                                        .strip_prefix("KUIPER-")
                                        .unwrap()
                                ),
                            );
                            ctx.layer();
                        });
                    }),
                area,
            );
        })?;

        if event::poll(std::time::Duration::from_millis(16))? {
            if let event::Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q')
                    || key.code == KeyCode::Char('Q')
                {
                    break;
                }
            }
        }
    }

    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
