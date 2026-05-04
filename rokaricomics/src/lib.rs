#![no_std]
mod helper;
mod parser;

use aidoku::{
	Chapter,
	DeepLinkHandler,
	DeepLinkResult,
	FilterValue,
	Home,
	HomeComponent,
	HomeLayout,
	Listing,
	ListingProvider,
	Manga,
	MangaPageResult,
	Page,
	PageDescriptionProvider,
	Result,
	Source,
	alloc::{String, Vec, vec},
	prelude::*,
};
use helper::{get_chapter_id_from_url, get_series_id_from_url};

struct RokaRiComics;

impl Source for RokaRiComics {
	fn new() -> Self { Self }

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		_filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		parser::parse_manga_list(page, query)
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

impl ListingProvider for RokaRiComics {
	fn get_manga_list(&self, _listing: Listing, page: i32) -> Result<MangaPageResult> {
		// Use the same list implementation as search without a query.
		parser::parse_manga_list(page, None)
	}
}

impl Home for RokaRiComics {
	fn get_home(&self) -> Result<HomeLayout> {
		// Load all manga by paginating home pages until a page has no entries.
		let mut page = 1;
		let mut all_entries: Vec<Manga> = Vec::new();
		loop {
			let result = self.get_search_manga_list(None, page, Vec::new())?;
			if result.entries.is_empty() { break; }
			all_entries.extend(result.entries);
			if !result.has_next_page { break; }
			page += 1;
		}
		// Deduplicate by manga key to avoid repeats across pages.
		let mut unique: Vec<Manga> = Vec::new();
		let mut seen_keys: Vec<String> = Vec::new();
		for m in all_entries {
			if seen_keys.iter().any(|k| k == &m.key) { continue; }
			seen_keys.push(m.key.clone());
			unique.push(m);
		}
		// Split into multiple components, each with up to 3 entries, like SirenScans.
		let mut components: Vec<HomeComponent> = Vec::new();
		let mut iter = unique.into_iter().map(|m| m.into());
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

impl DeepLinkHandler for RokaRiComics {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		if let Some(slug) = get_series_id_from_url(&url) {
			return Ok(Some(DeepLinkResult::Manga { key: slug }));
		}
		if let Some(cid) = get_chapter_id_from_url(&url) {
			// We don’t have a cheap way to infer the manga key from the chapter URL,
			// so we provide an empty manga_key and let Aidoku resolve via updates.
			return Ok(Some(DeepLinkResult::Chapter { manga_key: String::new(), key: cid }));
		}
		Ok(None)
	}
}

impl PageDescriptionProvider for RokaRiComics {
	fn get_page_description(&self, _page: Page) -> Result<String> {
		Ok(String::from(""))
	}
}

register_source!(
	RokaRiComics,
	ListingProvider,
	Home,
	PageDescriptionProvider,
	DeepLinkHandler
);
