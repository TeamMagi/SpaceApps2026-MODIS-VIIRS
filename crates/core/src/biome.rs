use std::fmt;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 8] = b"PSBIOME1";
const HEADER_LEN: usize = 12;
const CELL_DEG: f64 = 0.25;

static EMBEDDED: &[u8] = include_bytes!("../../../data/landcover/biome_0p25.rle");

/// Biome stratum derived from MODIS IGBP land cover (see `data/landcover/SOURCE.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Biome {
    NonBurnable,
    Forest,
    Shrubland,
    Savanna,
    Grassland,
    Cropland,
    Wetland,
    Tundra,
}

impl Biome {
    pub const ALL: [Biome; 8] = [
        Biome::NonBurnable,
        Biome::Forest,
        Biome::Shrubland,
        Biome::Savanna,
        Biome::Grassland,
        Biome::Cropland,
        Biome::Wetland,
        Biome::Tundra,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Biome::NonBurnable => "non_burnable",
            Biome::Forest => "forest",
            Biome::Shrubland => "shrubland",
            Biome::Savanna => "savanna",
            Biome::Grassland => "grassland",
            Biome::Cropland => "cropland",
            Biome::Wetland => "wetland",
            Biome::Tundra => "tundra",
        }
    }

    fn from_code(code: u8) -> Option<Biome> {
        Biome::ALL.get(code as usize).copied()
    }
}

pub fn is_burnable(biome: Biome) -> bool {
    biome != Biome::NonBurnable
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BiomeGridError {
    TooShort,
    BadMagic,
    BadDimensions { width: usize, height: usize },
    TruncatedRun,
    ZeroRun,
    InvalidClass(u8),
    CellCountMismatch { expected: usize, actual: usize },
}

impl fmt::Display for BiomeGridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(f, "biome grid shorter than its {HEADER_LEN}-byte header"),
            Self::BadMagic => write!(f, "biome grid magic is not PSBIOME1"),
            Self::BadDimensions { width, height } => {
                write!(f, "biome grid dimensions {width}x{height} are invalid")
            }
            Self::TruncatedRun => write!(f, "biome grid ends in the middle of a run"),
            Self::ZeroRun => write!(f, "biome grid contains a zero-length run"),
            Self::InvalidClass(c) => write!(f, "biome grid contains invalid class code {c}"),
            Self::CellCountMismatch { expected, actual } => {
                write!(f, "biome grid holds {actual} cells, expected {expected}")
            }
        }
    }
}

impl std::error::Error for BiomeGridError {}

/// Global biome grid, row 0 = northernmost, column 0 = 180°W.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiomeGrid {
    width: usize,
    height: usize,
    cells: Vec<u8>,
}

impl BiomeGrid {
    /// Decodes the run-length `PSBIOME1` format (u16 LE width, u16 LE height, then
    /// (u16 LE run, u8 class) pairs in row-major order), validating every field.
    pub fn decode(data: &[u8]) -> Result<BiomeGrid, BiomeGridError> {
        if data.len() < HEADER_LEN {
            return Err(BiomeGridError::TooShort);
        }
        if &data[..8] != MAGIC {
            return Err(BiomeGridError::BadMagic);
        }
        let width = u16::from_le_bytes([data[8], data[9]]) as usize;
        let height = u16::from_le_bytes([data[10], data[11]]) as usize;
        if width == 0 || height == 0 {
            return Err(BiomeGridError::BadDimensions { width, height });
        }
        let expected = width * height;
        let body = &data[HEADER_LEN..];
        if !body.len().is_multiple_of(3) {
            return Err(BiomeGridError::TruncatedRun);
        }
        let mut cells = Vec::with_capacity(expected);
        for off in (0..body.len()).step_by(3) {
            let len = u16::from_le_bytes([body[off], body[off + 1]]) as usize;
            let class = body[off + 2];
            if len == 0 {
                return Err(BiomeGridError::ZeroRun);
            }
            if Biome::from_code(class).is_none() {
                return Err(BiomeGridError::InvalidClass(class));
            }
            // Fail before resizing so a hostile file cannot allocate far beyond the declared size.
            if cells.len() + len > expected {
                return Err(BiomeGridError::CellCountMismatch {
                    expected,
                    actual: cells.len() + len,
                });
            }
            cells.resize(cells.len() + len, class);
        }
        if cells.len() != expected {
            return Err(BiomeGridError::CellCountMismatch {
                expected,
                actual: cells.len(),
            });
        }
        Ok(BiomeGrid {
            width,
            height,
            cells,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Biome of the cell containing `(lat, lon)`; `None` for non-finite input, `|lat| > 90`
    /// or `|lon| > 180`. Longitude 180 is the same meridian as -180.
    pub fn biome_at(&self, lat: f64, lon: f64) -> Option<Biome> {
        if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 {
            return None;
        }
        let lon = if lon >= 180.0 { lon - 360.0 } else { lon };
        let row = (((90.0 - lat) / CELL_DEG) as usize).min(self.height - 1);
        let col = (((lon + 180.0) / CELL_DEG) as usize).min(self.width - 1);
        Biome::from_code(self.cells[row * self.width + col])
    }
}

fn embedded_grid() -> &'static BiomeGrid {
    static GRID: OnceLock<BiomeGrid> = OnceLock::new();
    GRID.get_or_init(|| BiomeGrid::decode(EMBEDDED).expect("embedded biome grid is valid"))
}

/// Biome at `(lat, lon)` from the embedded 0.25° grid.
pub fn biome_at(lat: f64, lon: f64) -> Option<Biome> {
    embedded_grid().biome_at(lat, lon)
}

/// Majority biome of a set of `(lat, lon)` points. NonBurnable only wins when no
/// burnable point exists, because K14 needs the vegetation stratum of a region, not its coast.
/// Ties go to the biome with the lower code so the result is deterministic.
pub fn dominant_biome(points: impl Iterator<Item = (f64, f64)>) -> Option<Biome> {
    let mut counts = [0usize; 8];
    for (lat, lon) in points {
        if let Some(b) = biome_at(lat, lon) {
            counts[b as usize] += 1;
        }
    }
    let burnable = (1..8).max_by_key(|&i| (counts[i], std::cmp::Reverse(i)));
    match burnable {
        Some(i) if counts[i] > 0 => Biome::from_code(i as u8),
        _ if counts[0] > 0 => Some(Biome::NonBurnable),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(width: u16, height: u16, runs: &[(u16, u8)]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        for (len, class) in runs {
            out.extend_from_slice(&len.to_le_bytes());
            out.push(*class);
        }
        out
    }

    #[test]
    fn decodes_small_grid_with_runs_crossing_rows() {
        // 4x2 grid laid out as [0 0 1 1 / 1 5 5 5]: the second run crosses the row end.
        let data = encode(4, 2, &[(2, 0), (3, 1), (3, 5)]);
        let grid = BiomeGrid::decode(&data).unwrap();
        assert_eq!((grid.width(), grid.height()), (4, 2));
        assert_eq!(grid.biome_at(89.9, -179.9), Some(Biome::NonBurnable));
        assert_eq!(grid.biome_at(89.9, -179.7), Some(Biome::NonBurnable));
        assert_eq!(grid.biome_at(89.9, -179.4), Some(Biome::Forest));
        assert_eq!(grid.biome_at(89.6, -179.9), Some(Biome::Forest));
        assert_eq!(grid.biome_at(89.6, -179.7), Some(Biome::Cropland));
    }

    #[test]
    fn rejects_bad_input() {
        let good = encode(2, 2, &[(4, 1)]);
        assert!(BiomeGrid::decode(&good).is_ok());

        assert_eq!(BiomeGrid::decode(&good[..5]), Err(BiomeGridError::TooShort));

        let mut bad_magic = good.clone();
        bad_magic[0] = b'X';
        assert_eq!(BiomeGrid::decode(&bad_magic), Err(BiomeGridError::BadMagic));

        assert_eq!(
            BiomeGrid::decode(&good[..good.len() - 1]),
            Err(BiomeGridError::TruncatedRun)
        );
        assert_eq!(
            BiomeGrid::decode(&encode(2, 2, &[(3, 1)])),
            Err(BiomeGridError::CellCountMismatch {
                expected: 4,
                actual: 3
            })
        );
        assert!(matches!(
            BiomeGrid::decode(&encode(2, 2, &[(5, 1)])),
            Err(BiomeGridError::CellCountMismatch { expected: 4, .. })
        ));
        assert_eq!(
            BiomeGrid::decode(&encode(2, 2, &[(4, 8)])),
            Err(BiomeGridError::InvalidClass(8))
        );
        assert_eq!(
            BiomeGrid::decode(&encode(2, 2, &[(0, 1), (4, 1)])),
            Err(BiomeGridError::ZeroRun)
        );
        assert!(matches!(
            BiomeGrid::decode(&encode(0, 2, &[])),
            Err(BiomeGridError::BadDimensions { .. })
        ));
    }

    #[test]
    fn embedded_grid_is_global_quarter_degree() {
        let grid = embedded_grid();
        assert_eq!((grid.width(), grid.height()), (1440, 720));
        assert_eq!(grid.cells.len(), 1440 * 720);
    }

    #[test]
    fn known_locations() {
        assert_eq!(biome_at(-3.0, -62.0), Some(Biome::Forest));
        assert_eq!(biome_at(23.0, 12.0), Some(Biome::NonBurnable));
        assert_eq!(biome_at(0.0, -140.0), Some(Biome::NonBurnable));
        assert_eq!(biome_at(48.5, 32.0), Some(Biome::Cropland));
        assert_eq!(biome_at(30.8, 75.5), Some(Biome::Cropland));
        assert_eq!(biome_at(-15.0, 133.0), Some(Biome::Savanna));
        assert_eq!(biome_at(-13.0, -47.0), Some(Biome::Savanna));
        assert_eq!(biome_at(60.0, 100.0), Some(Biome::Forest));
        // The source marks the Konya cell as shrubland rather than cropland/grassland.
        assert_eq!(biome_at(38.0, 33.0), Some(Biome::Shrubland));
        for (lat, lon) in [(36.9, 30.7), (37.1, 28.4)] {
            assert!(is_burnable(biome_at(lat, lon).unwrap()), "{lat},{lon}");
        }
    }

    #[test]
    fn longitude_wraps_and_bad_input_is_none() {
        assert_eq!(biome_at(48.5, 180.0), biome_at(48.5, -180.0));
        assert!(biome_at(48.5, 180.0).is_some());
        assert_eq!(biome_at(90.0, 0.0), biome_at(89.9, 0.0));
        assert!(biome_at(-90.0, 0.0).is_some());
        assert_eq!(biome_at(f64::NAN, 0.0), None);
        assert_eq!(biome_at(0.0, f64::INFINITY), None);
        assert_eq!(biome_at(91.0, 0.0), None);
        assert_eq!(biome_at(0.0, 181.0), None);
    }

    #[test]
    fn dominant_biome_prefers_burnable_majority() {
        let amazon = (-3.0, -62.0);
        let ukraine = (48.5, 32.0);
        let ocean = (0.0, -140.0);
        let pts = [ocean, ocean, ocean, amazon, amazon, ukraine];
        assert_eq!(dominant_biome(pts.into_iter()), Some(Biome::Forest));
        assert_eq!(
            dominant_biome([ocean, ocean].into_iter()),
            Some(Biome::NonBurnable)
        );
        assert_eq!(dominant_biome(std::iter::empty()), None);
        assert_eq!(dominant_biome([(f64::NAN, 0.0)].into_iter()), None);
        // tie between Forest (1) and Cropland (5) goes to the lower code
        assert_eq!(
            dominant_biome([amazon, ukraine].into_iter()),
            Some(Biome::Forest)
        );
    }

    #[test]
    fn biome_names_and_serde() {
        assert_eq!(Biome::ALL.len(), 8);
        for (i, b) in Biome::ALL.iter().enumerate() {
            assert_eq!(*b as u8 as usize, i);
            assert_eq!(
                serde_json::to_string(b).unwrap(),
                format!("\"{}\"", b.as_str())
            );
        }
        assert!(!is_burnable(Biome::NonBurnable));
    }
}
