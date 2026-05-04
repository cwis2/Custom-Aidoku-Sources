use aidoku::{
	prelude::*,
	alloc::{string::{String, ToString}, vec::Vec},
	imports::net::{HttpMethod, Request},
	Manga, MangaPageResult, Chapter, Page, PageContent, MangaStatus, ContentRating, Result, Viewer,
};
use crate::helper::{BASE_URL, build_url_from_path, get_chapter_id_from_url};

pub fn parse_manga_list(page: i32, query: Option<String>) -> Result<MangaPageResult> {
	// When there is a query, use the built-in search: `/?s=query`.
	// Otherwise, paginate the home pages: `/`, `/page/2/`, `/page/3/`, ...
	let url = if let Some(q) = &query {
		format!("{}/?s={}", BASE_URL, q)
	} else if page <= 1 {
		String::from(BASE_URL)
	} else {
		format!("{}/page/{}/", BASE_URL, page)
	};

	let html = match Request::new(url.clone(), HttpMethod::Get) {
		Ok(req) => match req.html() {
			Ok(h) => h,
			// If the page doesn't exist or fails to load, treat it as an empty result.
			Err(_e) => {
				return Ok(MangaPageResult { entries: Vec::new(), has_next_page: false });
			}
		},
		Err(_e) => {
			return Ok(MangaPageResult { entries: Vec::new(), has_next_page: false });
		}
	};
	let mut entries: Vec<Manga> = Vec::new();

	// Cards like: <div class="bs styletere stylefiv"><div class="bsx"> ... </div></div>
	if let Some(mut cards) = html.select(".bs.styletere.stylefiv, .bsx") {
		for card in &mut cards {
			// Prefer inner .bsx anchor
			let series_link = card
				.select_first("a[href*='/manga/']")
				.or_else(|| card.parent().and_then(|p| p.select_first("a[href*='/manga/']")));
			let href = series_link.as_ref().and_then(|a| a.attr("href")).unwrap_or_default();
			if href.is_empty() { continue; }
			let mut path = href.to_string();
			if let Some(idx) = path.find("https://") {
				// full URL, keep path only
				if let Some(p) = path[idx..].find("/manga/") {
					path = path[idx + p + 1..].to_string();
				}
			}
			// Key as path without domain, no leading slash
			let key = path.trim_matches('/').to_string();
			if key.is_empty() { continue; }

			let title = series_link
				.as_ref()
				.and_then(|a| a.attr("title"))
				.or_else(|| series_link.as_ref().and_then(|a| a.text()))
				.unwrap_or_else(|| key.clone());

			// Cover image inside <div class="limit"><img ...>
			let cover = card
				.select_first("img.ts-post-image, img")
				.and_then(|img| img.attr("src"))
				.map(|s| s.to_string());

			let url = Some(build_url_from_path(&key));

			entries.push(Manga {
				key,
				title,
				cover,
				url,
				viewer: Viewer::Webtoon,
				..Default::default()
			});
		}
	}

	// If this page returned entries, assume there *might* be a next page.
	// The caller (e.g. Home) will stop once a later page returns empty.
	let has_next_page = !entries.is_empty();
	Ok(MangaPageResult { entries, has_next_page })
}

pub fn parse_manga_details(key: String) -> Result<Manga> {
	let url = if key.starts_with("manga/") {
		build_url_from_path(&key)
	} else {
		// If we somehow only got the slug portion, prepend manga/
		build_url_from_path(&format!("manga/{}", key))
	};
	let html = match Request::new(url.clone(), HttpMethod::Get) {
		Ok(req) => match req.html() {
			Ok(h) => h,
			Err(e) => { return Err(aidoku::AidokuError::RequestError(e)); }
		},
		Err(e) => { return Err(aidoku::AidokuError::RequestError(e)); }
	};

	let body = html.select_first("body");

	// Title: <h1 class="entry-title" itemprop="name">...</h1>
	let title = body
		.as_ref()
		.and_then(|b| b.select_first("h1.entry-title"))
		.and_then(|e| e.text())
		.unwrap_or_else(|| key.clone());

	// Cover: main post img wp-post-image
	let cover = body
		.as_ref()
		.and_then(|b| b.select_first("img.wp-post-image, img[ itemprop='image' ]"))
		.and_then(|e| e.attr("src"))
		.map(|s| s.to_string());

	// Description: <div class="entry-content entry-content-single" itemprop="description"> ...
	let description = body
		.as_ref()
		.and_then(|b| b.select_first(".entry-content.entry-content-single"))
		.and_then(|e| e.text())
		.filter(|t| !t.is_empty());

	// Genres: <div class="seriestugenre"><a ...>Genre</a> ...</div>
	let mut tags: Vec<String> = Vec::new();
	if let Some(container) = body.as_ref().and_then(|b| b.select_first(".seriestugenre")) {
		if let Some(mut gens) = container.select("a[rel='tag']") {
			for g in &mut gens {
				if let Some(t) = g.text() {
					if !t.is_empty() { tags.push(t); }
				}
			}
		}
	}

	// Status: from infotable row where first td is "Status"
	let mut status = MangaStatus::Unknown;
	if let Some(info) = body.as_ref().and_then(|b| b.select_first("table.infotable")) {
		if let Some(mut rows) = info.select("tr") {
			for row in &mut rows {
				let first = row.select_first("td");
				let second = first
					.as_ref()
					.and_then(|_| row.select("td"))
					.and_then(|mut cells| {
						let mut last = None;
						for c in &mut cells { last = Some(c); }
						last
					});
				let key_text = first.and_then(|c| c.text()).unwrap_or_default().to_lowercase();
				if key_text.contains("status") {
					let val = second.and_then(|c| c.text()).unwrap_or_default().to_lowercase();
					if val.contains("ongoing") { status = MangaStatus::Ongoing; }
					else if val.contains("complete") { status = MangaStatus::Completed; }
					else if val.contains("hiatus") { status = MangaStatus::Hiatus; }
					else if val.contains("drop") { status = MangaStatus::Cancelled; }
				}
			}
		}
	}

	let mut content_rating = ContentRating::Safe;
	for t in &tags {
		let lc = t.to_ascii_lowercase();
		if lc.contains("adult") || lc.contains("smut") || lc.contains("nsfw") {
			content_rating = ContentRating::Suggestive;
		}
	}

	Ok(Manga {
		key,
		title,
		cover,
		url: Some(url),
		description,
		authors: None,
		artists: None,
		tags: if tags.is_empty() { None } else { Some(tags) },
		status,
		content_rating,
		viewer: Viewer::Webtoon,
		..Default::default()
	})
}

pub fn parse_chapter_list(key: String) -> Result<Vec<Chapter>> {
	let url = if key.starts_with("manga/") {
		build_url_from_path(&key)
	} else {
		build_url_from_path(&format!("manga/{}", key))
	};
	let html = match Request::new(url.clone(), HttpMethod::Get) {
		Ok(req) => match req.html() {
			Ok(h) => h,
			Err(e) => { return Err(aidoku::AidokuError::RequestError(e)); }
		},
		Err(e) => { return Err(aidoku::AidokuError::RequestError(e)); }
	};
	let mut chapters: Vec<Chapter> = Vec::new();

	// Chapter list in <ul><li data-num="27"> ...</li></ul>
	if let Some(mut lis) = html.select("ul li[data-num]") {
		for li in &mut lis {
			let a = li.select_first("a[href]");
			let href = a.as_ref().and_then(|e| e.attr("href")).unwrap_or_default();
			if href.is_empty() { continue; }
			let cid = get_chapter_id_from_url(&href).unwrap_or_default();
			if cid.is_empty() { continue; }
			let chap_url = build_url_from_path(&cid);

			// Skip chapters that are paywalled. These pages render a `.lock-container` instead
			// of the normal reader, so they will never load properly in Aidoku.
			if let Ok(req) = Request::new(chap_url.clone(), HttpMethod::Get) {
				if let Ok(ch_html) = req.html() {
					if ch_html.select_first(".lock-container").is_some() {
						continue;
					}
				}
			}

			let raw_title = a
				.as_ref()
				.and_then(|e| e.select_first(".chapternum, .fivchap"))
				.and_then(|e| e.text())
				.unwrap_or_else(|| cid.clone());
			let cleaned_title = raw_title.split_whitespace().collect::<Vec<_>>().join(" ");
			let chapter_number = cleaned_title
				.to_lowercase()
				.replace("chapter", "")
				.replace("ch.", "")
				.trim()
				.parse::<f32>()
				.ok();

			let chapter = Chapter {
				key: cid,
				title: Some(String::from(cleaned_title.trim())),
				chapter_number,
				date_uploaded: None,
				url: Some(chap_url),
				..Default::default()
			};
			chapters.push(chapter);
		}
	}

	Ok(chapters)
}

pub fn parse_page_list(chapter_key: String) -> Result<Vec<Page>> {
	let url = if chapter_key.starts_with("http://") || chapter_key.starts_with("https://") {
		chapter_key.clone()
	} else {
		build_url_from_path(&chapter_key)
	};
	let html = match Request::new(url.clone(), HttpMethod::Get) {
		Ok(req) => match req.html() {
			Ok(h) => h,
			Err(e) => { return Err(aidoku::AidokuError::RequestError(e)); }
		},
		Err(e) => { return Err(aidoku::AidokuError::RequestError(e)); }
	};
	let mut pages: Vec<Page> = Vec::new();

	// Images are in <div id="readerarea"><img class="ts-main-image" ...></div>
	if let Some(reader) = html.select_first("#readerarea") {
		if let Some(mut imgs) = reader.select("img.ts-main-image, img") {
			for img in &mut imgs {
				let src = img.attr("src").unwrap_or_default();
				if src.is_empty() { continue; }
				pages.push(Page {
					content: PageContent::url(src),
					..Default::default()
				});
			}
		}
	}

	Ok(pages)
}
