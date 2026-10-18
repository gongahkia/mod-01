//! Native Studio and standalone games use one executable and one runtime path.
use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Eq, PartialEq)]
pub enum Launch {
    Studio(Option<PathBuf>),
    Player(PathBuf),
    Verify { path: PathBuf, frames: u32 },
    Help,
}

pub fn parse(args: &[OsString], bundled_game: Option<PathBuf>) -> Result<Launch, String> {
    match args {
        [] => Ok(bundled_game.map_or(Launch::Studio(None), Launch::Player)),
        [flag] if flag == "--help" || flag == "-h" => Ok(Launch::Help),
        [flag, path] if flag == "--play" => Ok(Launch::Player(path.into())),
        [flag, path] if flag == "--studio" => Ok(Launch::Studio(Some(path.into()))),
        [flag, path] if flag == "--verify" => Ok(Launch::Verify {
            path: path.into(),
            frames: 3,
        }),
        [flag, path, count_flag, count] if flag == "--verify" && count_flag == "--frames" => {
            let frames = count
                .to_str()
                .and_then(|count| count.parse::<u32>().ok())
                .filter(|count| (1..=36_000).contains(count))
                .ok_or("--frames must be 1-36000")?;
            Ok(Launch::Verify {
                path: path.into(),
                frames,
            })
        }
        [path] if !path.to_string_lossy().starts_with('-') => Ok(Launch::Studio(Some(path.into()))),
        _ => {
            Err("usage: mod01-studio [PATH | --play PATH | --verify PATH [--frames N]]".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    #[test]
    fn packaged_games_autolaunch_but_explicit_studio_preserves_source_access() {
        let bundled = PathBuf::from("game.m01c");
        assert_eq!(
            parse(&[], Some(bundled.clone())).unwrap(),
            Launch::Player(bundled)
        );
        assert_eq!(
            parse(&args(&["--studio", "game.m01c"]), None).unwrap(),
            Launch::Studio(Some("game.m01c".into()))
        );
        assert_eq!(
            parse(&args(&["--play", "game.m01c"]), None).unwrap(),
            Launch::Player("game.m01c".into())
        );
        assert_eq!(parse(&[], None).unwrap(), Launch::Studio(None));
    }
    #[test]
    fn verification_is_bounded_and_unknown_flags_are_errors() {
        assert_eq!(
            parse(&args(&["--verify", "game.m01c", "--frames", "4"]), None).unwrap(),
            Launch::Verify {
                path: "game.m01c".into(),
                frames: 4
            }
        );
        for count in ["0", "36001", "-1", "nan"] {
            assert!(parse(&args(&["--verify", "game.m01c", "--frames", count]), None).is_err());
        }
        assert!(parse(&args(&["--typo"]), None).is_err());
    }
}
