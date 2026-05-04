#![no_std]
mod helper;
mod parser;

use aidoku::{
    alloc::{vec, String, Vec, string::ToString},
    prelude::*,
    Chapter, DeepLinkHandler, DeepLinkResult,
    FilterValue, Home, HomeComponent, HomeLayout, Listing, ListingProvider, Manga,
    MangaPageResult, Page, PageDescriptionProvider,
    Result, Source,
};
use helper::{get_chapter_id_from_url, get_series_id_from_url};

struct SirenScans;

impl Source for SirenScans {
    fn new() -> Self { Self }

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        _filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {
        let mut result = parser::parse_manga_list(page)?;
        if let Some(q) = query {
            let ql = q.to_ascii_lowercase();
            result.entries.retain(|m| m.title.to_ascii_lowercase().contains(&ql));
        }
        Ok(result)
    }

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {
        let key = manga.key.clone();
        if needs_details {
            manga = parser::parse_manga_details(key.clone())?;
        }
        if needs_chapters {
            manga.chapters = Some(parser::parse_chapter_list(key)?);
        }
        Ok(manga)
    }

    fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
        parser::parse_page_list(chapter.key)
    }
}

impl ListingProvider for SirenScans {
    fn get_manga_list(&self, _listing: Listing, page: i32) -> Result<MangaPageResult> {
        parser::parse_manga_list(page)
    }
}

impl DeepLinkHandler for SirenScans {
    fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
        if let Some(slug) = get_series_id_from_url(&url) {
            return Ok(Some(DeepLinkResult::Manga { key: slug }));
        }
        if let Some(cid) = get_chapter_id_from_url(&url) {
            // SirenScans chapter id format is `{seriesid}-{chapterid}`
            let manga_key = cid.split('-').next().unwrap_or("").to_string();
            return Ok(Some(DeepLinkResult::Chapter { manga_key, key: cid }));
        }
        Ok(None)
    }
}

impl Home for SirenScans {
    fn get_home(&self) -> Result<HomeLayout> {
        // Load all manga by paginating until no next page
        let mut page = 1;
        let mut all_entries: Vec<Manga> = Vec::new();
        loop {
            let result = self.get_search_manga_list(None, page, Vec::new())?;
            if result.entries.is_empty() { break; }
            all_entries.extend(result.entries);
            if !result.has_next_page { break; }
            page += 1;
        }
        // Split into multiple components of 3 entries each
        let mut components: Vec<HomeComponent> = Vec::new();
        let mut iter = all_entries.into_iter().map(|m| m.into());
        loop {
            let mut entries = Vec::new();
            for _ in 0..3 {
                if let Some(item) = iter.next() { entries.push(item); } else { break; }
            }
            if entries.is_empty() { break; }
            components.push(HomeComponent {
                title: None,
                subtitle: None,
                value: aidoku::HomeComponentValue::Scroller {
                    entries,
                    listing: None,
                },
            });
        }
        Ok(HomeLayout { components })
    }
}

impl PageDescriptionProvider for SirenScans {
    fn get_page_description(&self, _page: Page) -> Result<String> {
        Ok(String::from(""))
    }
}

register_source!(
    SirenScans,
    ListingProvider,
    Home,
    PageDescriptionProvider,
    DeepLinkHandler
);
