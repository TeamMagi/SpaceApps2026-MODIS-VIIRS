use anyhow::{Context, Result};
use pyrosync_core::models::{BBox, Hotspot};
use reqwest::Client;

const FIRMS_BASE_URL: &str = "https://firms.modaps.eosdis.nasa.gov/api/area/csv";

pub struct FirmsClient {
    http: Client,
    api_key: String,
}

impl FirmsClient {
    pub fn new(api_key: String) -> Self {
        Self {
            http: Client::new(),
            api_key,
        }
    }

    pub async fn fetch_hotspots(
        &self,
        source: &str,
        bbox: &BBox,
        days: u32,
    ) -> Result<Vec<Hotspot>> {
        let url = format!(
            "{}/{}/{}/{}/{}/{}",
            FIRMS_BASE_URL,
            self.api_key,
            source,
            bbox.to_firms_string(),
            days,
            chrono::Utc::now().format("%Y-%m-%d"),
        );

        let text = self
            .http
            .get(&url)
            .send()
            .await
            .context("FIRMS API request failed")?
            .text()
            .await
            .context("Failed to read FIRMS response")?;

        crate::parse::parse_csv(&text, source)
    }

    pub async fn fetch_modis(&self, bbox: &BBox, days: u32) -> Result<Vec<Hotspot>> {
        self.fetch_hotspots("MODIS_NRT", bbox, days).await
    }

    pub async fn fetch_viirs_snpp(&self, bbox: &BBox, days: u32) -> Result<Vec<Hotspot>> {
        self.fetch_hotspots("VIIRS_SNPP_NRT", bbox, days).await
    }

    pub async fn fetch_viirs_noaa20(&self, bbox: &BBox, days: u32) -> Result<Vec<Hotspot>> {
        self.fetch_hotspots("VIIRS_NOAA20_NRT", bbox, days).await
    }

    pub async fn fetch_viirs_noaa21(&self, bbox: &BBox, days: u32) -> Result<Vec<Hotspot>> {
        self.fetch_hotspots("VIIRS_NOAA21_NRT", bbox, days).await
    }

    pub async fn fetch_all(&self, bbox: &BBox, days: u32) -> Result<(Vec<Hotspot>, Vec<Hotspot>)> {
        let (modis, snpp, noaa20, noaa21) = tokio::try_join!(
            self.fetch_modis(bbox, days),
            self.fetch_viirs_snpp(bbox, days),
            self.fetch_viirs_noaa20(bbox, days),
            self.fetch_viirs_noaa21(bbox, days),
        )?;

        let mut viirs = snpp;
        viirs.extend(noaa20);
        viirs.extend(noaa21);

        Ok((modis, viirs))
    }
}
