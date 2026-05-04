use aidoku::{ prelude::format, alloc::string::String };

pub const BASE_URL: &str = "https://sirenscans.com";

pub fn build_series_url(slug: &str) -> String {
	format!("{}/series/{}/", BASE_URL, slug.trim_matches('/'))
}

pub fn build_chapter_url(id: &str) -> String {
	// SirenScans chapter id pattern is `{seriesid}-{chapterid}`
	format!("{}/chapter/{}/", BASE_URL, id.trim_matches('/'))
}

pub fn get_series_id_from_url(url: &str) -> Option<String> {
	let lower = url.to_lowercase();
	if let Some(pos) = lower.find("/series/") {
		let mut slug = &url[pos + "/series/".len()..];
		if let Some(end) = slug.find('/') { slug = &slug[..end]; }
		return Some(String::from(slug));
	}
	None
}

pub fn get_chapter_id_from_url(url: &str) -> Option<String> {
	let lower = url.to_lowercase();
	if let Some(pos) = lower.find("/chapter/") {
		let mut cid = &url[pos + "/chapter/".len()..];
		if let Some(end) = cid.find('/') { cid = &cid[..end]; }
		return Some(String::from(cid));
	}
	None
}
