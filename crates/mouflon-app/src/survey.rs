//! Ce que le processus auxiliaire du relevé (voir [`crate::helper`]) envoie
//! à l'app, et ce qu'elle lui demande : des lignes de texte, faciles à lire
//! dans un journal et à vérifier dans un test.
//!
//! Un relevé :
//!
//! ```text
//! R <n°> <app au premier plan|-> <autorisé 0|1> <depuis la touche|-> <inactif|-> <apps suivies> <µs fenêtres> <µs Dock> <signal 0|1>
//! D <x> <y> <l> <h>                                  (le Dock, s'il est lu)
//! W <id> <pid> <niveau> <x> <y> <l> <h> <propriétaire>  (une ligne par fenêtre)
//! .
//! ```
//!
//! Les demandes de l'app : `vite 1` ou `vite 0` (relever souvent ou au
//! calme), `dock 1` ou `dock 0` (lire le cadre du Dock), `maintenant` (tout
//! relire tout de suite, Dock et autorisation compris).

use std::fmt::Write as _;
use std::time::Duration;

use mouflon_platform::{OsRect, OsWindow, UserActivity};

/// Argument qui lance le programme en processus auxiliaire du relevé.
pub const HELPER_ARG: &str = "--releve";

/// Un relevé du système.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Survey {
    /// Numéro du relevé (le premier vaut 1).
    pub seq: u64,
    /// Fenêtres visibles des autres apps, de la plus en avant à la plus en arrière.
    pub windows: Vec<OsWindow>,
    /// Cadre des icônes du Dock, s'il est demandé et lisible.
    pub dock: Option<OsRect>,
    /// Processus de l'app au premier plan.
    pub frontmost: Option<i32>,
    /// Mouflon a-t-il l'autorisation d'Accessibilité ?
    pub trusted: bool,
    /// Ce que fait l'utilisateur, au moment du relevé.
    pub activity: Option<UserActivity>,
    /// Apps dont les fenêtres qui bougent sont signalées par l'Accessibilité.
    pub followed: u32,
    /// Durée des questions au système (pour le journal des essais).
    pub windows_took: Duration,
    pub dock_took: Duration,
    /// Relevé fait sur le signal d'une fenêtre qui bouge.
    pub nudged: bool,
}

/// Une demande de l'app au processus auxiliaire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Relever souvent (une fenêtre qui porte un animal bouge, on en tient un).
    Fast(bool),
    /// Lire aussi le cadre du Dock (« Marcher sous le Dock »).
    Dock(bool),
    /// Tout relire tout de suite.
    Now,
}

impl Command {
    /// La ligne qui la porte (sans retour à la ligne).
    #[must_use]
    pub fn line(self) -> &'static str {
        match self {
            Self::Fast(true) => "vite 1",
            Self::Fast(false) => "vite 0",
            Self::Dock(true) => "dock 1",
            Self::Dock(false) => "dock 0",
            Self::Now => "maintenant",
        }
    }

    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        match line.trim() {
            "vite 1" => Some(Self::Fast(true)),
            "vite 0" => Some(Self::Fast(false)),
            "dock 1" => Some(Self::Dock(true)),
            "dock 0" => Some(Self::Dock(false)),
            "maintenant" => Some(Self::Now),
            _ => None,
        }
    }
}

fn opt<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

impl Survey {
    /// Ajoute le relevé à `out`, ligne par ligne, jusqu'au point final.
    pub fn encode(&self, out: &mut String) {
        let _ = writeln!(
            out,
            "R {} {} {} {} {} {} {} {} {}",
            self.seq,
            opt(self.frontmost),
            u8::from(self.trusted),
            opt(self.activity.map(|a| a.since_key)),
            opt(self.activity.map(|a| a.idle)),
            self.followed,
            self.windows_took.as_micros(),
            self.dock_took.as_micros(),
            u8::from(self.nudged),
        );
        if let Some(d) = self.dock {
            let _ = writeln!(out, "D {} {} {} {}", d.x, d.y, d.w, d.h);
        }
        for w in &self.windows {
            // Le nom du propriétaire est en dernier, espaces compris.
            let owner: String = w
                .owner
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            let _ = writeln!(
                out,
                "W {} {} {} {} {} {} {} {}",
                w.id, w.pid, w.layer, w.frame.x, w.frame.y, w.frame.w, w.frame.h, owner
            );
        }
        out.push_str(".\n");
    }
}

/// Relit les relevés, ligne par ligne.
#[derive(Debug, Default)]
pub struct Reader {
    current: Option<Survey>,
}

impl Reader {
    /// Une ligne reçue ; le relevé quand c'est sa dernière. Une ligne
    /// illisible est une erreur (le relevé en cours est abandonné).
    pub fn line(&mut self, line: &str) -> Result<Option<Survey>, String> {
        let line = line.trim_end_matches(['\n', '\r']);
        let result = self.parse(line);
        if result.is_err() {
            self.current = None;
        }
        result
    }

    fn parse(&mut self, line: &str) -> Result<Option<Survey>, String> {
        let bad = || format!("ligne illisible : {line:?}");
        let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
        match kind {
            "R" => {
                let f: Vec<&str> = rest.split(' ').collect();
                if f.len() != 9 {
                    return Err(bad());
                }
                let num = |s: &str| s.parse::<f64>().map_err(|_| bad());
                let maybe = |s: &str| -> Result<Option<f64>, String> {
                    if s == "-" { Ok(None) } else { num(s).map(Some) }
                };
                let flag = |s: &str| match s {
                    "0" => Ok(false),
                    "1" => Ok(true),
                    _ => Err(bad()),
                };
                let micros = |s: &str| {
                    s.parse::<u64>()
                        .map(Duration::from_micros)
                        .map_err(|_| bad())
                };
                let since_key = maybe(f[3])?;
                let idle = maybe(f[4])?;
                self.current = Some(Survey {
                    seq: f[0].parse().map_err(|_| bad())?,
                    frontmost: if f[1] == "-" {
                        None
                    } else {
                        Some(f[1].parse().map_err(|_| bad())?)
                    },
                    trusted: flag(f[2])?,
                    activity: since_key
                        .zip(idle)
                        .map(|(since_key, idle)| UserActivity { since_key, idle }),
                    followed: f[5].parse().map_err(|_| bad())?,
                    windows_took: micros(f[6])?,
                    dock_took: micros(f[7])?,
                    nudged: flag(f[8])?,
                    ..Survey::default()
                });
                Ok(None)
            }
            "D" => {
                let survey = self.current.as_mut().ok_or_else(bad)?;
                let v: Vec<f64> = rest
                    .split(' ')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad())?;
                let [x, y, w, h] = v[..] else {
                    return Err(bad());
                };
                survey.dock = Some(OsRect { x, y, w, h });
                Ok(None)
            }
            "W" => {
                let survey = self.current.as_mut().ok_or_else(bad)?;
                let mut f = rest.splitn(8, ' ');
                let mut next = || f.next().ok_or_else(bad);
                let id = next()?.parse().map_err(|_| bad())?;
                let pid = next()?.parse().map_err(|_| bad())?;
                let layer = next()?.parse().map_err(|_| bad())?;
                let mut coord = || -> Result<f64, String> { next()?.parse().map_err(|_| bad()) };
                let frame = OsRect {
                    x: coord()?,
                    y: coord()?,
                    w: coord()?,
                    h: coord()?,
                };
                let owner = f.next().unwrap_or_default().to_owned();
                survey.windows.push(OsWindow {
                    id,
                    pid,
                    frame,
                    layer,
                    owner,
                });
                Ok(None)
            }
            "." => self.current.take().map(Some).ok_or_else(bad),
            _ => Err(bad()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Survey {
        Survey {
            seq: 42,
            windows: vec![
                OsWindow {
                    id: 7,
                    pid: 501,
                    frame: OsRect {
                        x: -1440.5,
                        y: 25.0,
                        w: 800.25,
                        h: 600.0,
                    },
                    layer: 0,
                    owner: "Visualiseur de clavier".into(),
                },
                OsWindow {
                    id: 9,
                    pid: 77,
                    frame: OsRect {
                        x: 0.1,
                        y: 0.2,
                        w: 1.0 / 3.0,
                        h: 64.0,
                    },
                    layer: 1500,
                    owner: "nom\navec retour".into(),
                },
            ],
            dock: Some(OsRect {
                x: 300.0,
                y: 1050.0,
                w: 900.0,
                h: 64.0,
            }),
            frontmost: Some(501),
            trusted: true,
            activity: Some(UserActivity {
                since_key: 1.25,
                idle: 0.0625,
            }),
            followed: 3,
            windows_took: Duration::from_micros(1234),
            dock_took: Duration::from_micros(56),
            nudged: true,
        }
    }

    fn read_all(text: &str) -> Vec<Survey> {
        let mut reader = Reader::default();
        text.lines()
            .filter_map(|line| reader.line(line).expect("ligne lisible"))
            .collect()
    }

    #[test]
    fn a_survey_reads_back_exactly() {
        let mut text = String::new();
        let survey = sample();
        survey.encode(&mut text);
        let back = read_all(&text);
        let mut expected = survey;
        // Les caractères de contrôle d'un nom deviennent des espaces.
        expected.windows[1].owner = "nom avec retour".into();
        assert_eq!(back, vec![expected]);
    }

    #[test]
    fn an_empty_survey_and_several_in_a_row() {
        let mut text = String::new();
        let empty = Survey {
            seq: 1,
            ..Survey::default()
        };
        empty.encode(&mut text);
        sample().encode(&mut text);
        let back = read_all(&text);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0], empty);
        assert_eq!(back[1].windows.len(), 2);
    }

    #[test]
    fn a_bad_line_drops_the_survey_but_not_the_next() {
        let mut reader = Reader::default();
        assert!(reader.line("R 1 - 0 - - 0 0 0 0").unwrap().is_none());
        assert!(reader.line("W pas un nombre").is_err());
        assert!(reader.line(".").is_err(), "relevé abandonné");
        let mut text = String::new();
        sample().encode(&mut text);
        let mut got = None;
        for line in text.lines() {
            got = got.or(reader.line(line).unwrap());
        }
        assert_eq!(got.map(|s| s.seq), Some(42));
    }

    #[test]
    fn commands_round_trip() {
        for command in [
            Command::Fast(true),
            Command::Fast(false),
            Command::Dock(true),
            Command::Dock(false),
            Command::Now,
        ] {
            assert_eq!(Command::parse(command.line()), Some(command));
        }
        assert_eq!(Command::parse("n'importe quoi"), None);
    }
}
