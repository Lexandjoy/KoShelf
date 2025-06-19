use crate::models::{EpubInfo, Identifier};
use anyhow::{Result, Context, anyhow};
use std::path::Path;
use std::io::Read;
use zip::ZipArchive;
use std::fs::File;
use log::{debug, warn};

pub struct EpubParser;

impl EpubParser {
    pub fn new() -> Self {
        Self
    }
    
    pub async fn parse(&self, epub_path: &Path) -> Result<EpubInfo> {
        debug!("Opening EPUB: {:?}", epub_path);
        let file = File::open(epub_path).with_context(|| format!("Failed to open EPUB file: {:?}", epub_path))?;
        let mut zip = ZipArchive::new(file).with_context(|| format!("Failed to read EPUB as zip: {:?}", epub_path))?;

        // Step 1: Find the OPF file path from META-INF/container.xml
        let opf_path = {
            let mut container_xml = String::new();
            let mut container_file = zip.by_name("META-INF/container.xml")
                .with_context(|| "META-INF/container.xml not found in EPUB")?;
            container_file.read_to_string(&mut container_xml)?;
            Self::find_opf_path(&container_xml)?
        };
        debug!("Found OPF file path: {}", opf_path);

        // Step 2: Read the OPF file (scope the borrow)
        let opf_xml = {
            let mut opf_xml = String::new();
            {
                let mut opf_file = zip.by_name(&opf_path)
                    .with_context(|| format!("OPF file '{}' not found in EPUB", opf_path))?;
                opf_file.read_to_string(&mut opf_xml)?;
            }
            opf_xml
        };

        // Step 3: Parse OPF metadata
        let (epub_info, cover_id) = Self::parse_opf_metadata(&opf_xml)?;

       // Step 4: Find cover image path and MIME type in manifest
       let (cover_path, cover_mime_type) = Self::find_cover_path(&opf_xml, &cover_id)?;
       debug!("Cover image path: {:?}, MIME type: {:?}", cover_path, cover_mime_type);

       // Step 4.5: Resolve cover path relative to OPF directory
       let resolved_cover_path = if let Some(ref cover_path) = cover_path {
           use std::path::Path;
           let opf_parent = Path::new(&opf_path).parent();
           let joined = if let Some(parent) = opf_parent {
               parent.join(cover_path)
           } else {
               Path::new(cover_path).to_path_buf()
           };
           Some(joined.to_string_lossy().replace('\\', "/"))
       } else {
           None
       };

       // Step 5: Extract cover image bytes
       let cover_data = if let Some(ref cover_path) = resolved_cover_path {
           match zip.by_name(cover_path) {
               Ok(mut cover_file) => {
                   let mut buf = Vec::new();
                   cover_file.read_to_end(&mut buf)?;
                   Some(buf)
               },
               Err(e) => {
                   warn!("Cover image file '{}' not found: {}", cover_path, e);
                   None
               }
           }
       } else {
           None
       };
        
        Ok(EpubInfo {
            cover_data,
            cover_mime_type,
            ..epub_info
        })
    }

    fn find_opf_path(container_xml: &str) -> Result<String> {
        use quick_xml::Reader;
        use quick_xml::events::Event;
        let mut reader = Reader::from_str(container_xml);
        reader.trim_text(true);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                    if e.local_name().as_ref() == b"rootfile" {
                        for attr in e.attributes().flatten() {
                            let key = attr.key.as_ref();
                            if key == b"full-path" {
                                return Ok(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow!("Error parsing container.xml: {}", e)),
                _ => {}
            }
            buf.clear();
        }
        Err(anyhow!("No rootfile/full-path found in container.xml"))
    }

    fn parse_opf_metadata(opf_xml: &str) -> Result<(EpubInfo, Option<String>)> {
        use quick_xml::Reader;
        use quick_xml::events::Event;
        let mut reader = Reader::from_str(opf_xml);
        reader.trim_text(true);
        let mut buf = Vec::new();
        let mut in_metadata = false;
        let mut title = None;
        let mut authors = Vec::new();
        let mut description = None;
        let mut publisher = None;
        let mut language = None;
        let mut identifiers = Vec::new();
        let mut subjects = Vec::new();
        let mut meta_cover_id: Option<String> = None;
        let mut series = None;
        let mut series_number = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = e.local_name();
                    match name.as_ref() {
                        b"metadata" => in_metadata = true,
                        b"title" if in_metadata => {
                            if let Ok(Event::Text(text)) = reader.read_event_into(&mut buf) {
                                title = Some(text.unescape().unwrap_or_default().to_string());
                            }
                        },
                        b"creator" if in_metadata => {
                            if let Ok(Event::Text(text)) = reader.read_event_into(&mut buf) {
                                authors.push(text.unescape().unwrap_or_default().to_string());
                            }
                        },
                        b"description" if in_metadata => {
                            if let Ok(Event::Text(text)) = reader.read_event_into(&mut buf) {
                                description = Some(text.unescape().unwrap_or_default().to_string());
                            }
                        },
                        b"publisher" if in_metadata => {
                            if let Ok(Event::Text(text)) = reader.read_event_into(&mut buf) {
                                publisher = Some(text.unescape().unwrap_or_default().to_string());
                            }
                        },
                        b"language" if in_metadata => {
                            if let Ok(Event::Text(text)) = reader.read_event_into(&mut buf) {
                                language = Some(text.unescape().unwrap_or_default().to_string());
                            }
                        },
                        b"identifier" if in_metadata => {
                            let mut scheme = None;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"opf:scheme" || key == b"scheme" {
                                    scheme = Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                            if let Ok(Event::Text(text)) = reader.read_event_into(&mut buf) {
                                let value = text.unescape().unwrap_or_default().to_string();
                                let (final_scheme, final_value) = if let Some(s) = scheme {
                                    (s, value.clone())
                                } else if let Some(colon_pos) = value.find(':') {
                                    let potential_scheme = &value[..colon_pos];
                                    let potential_value = &value[colon_pos + 1..];
                                    (potential_scheme.to_string(), potential_value.to_string())
                                } else {
                                    ("unknown".to_string(), value.clone())
                                };
                                identifiers.push(Identifier::new(final_scheme, final_value));
                            }
                        },
                        b"subject" if in_metadata => {
                            if let Ok(Event::Text(text)) = reader.read_event_into(&mut buf) {
                                let subject = text.unescape().unwrap_or_default().to_string();
                                if !subject.is_empty() {
                                    subjects.push(subject);
                                }
                            }
                        },
                        b"meta" if in_metadata => {
                            let mut name = None;
                            let mut content = None;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"name" {
                                    name = Some(String::from_utf8_lossy(&attr.value).to_string());
                                } else if key == b"content" {
                                    content = Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                            if let (Some(n), Some(c)) = (name, content) {
                                if n == "cover" {
                                    meta_cover_id = Some(c.clone());
                                }
                                if n == "calibre:series" {
                                    series = Some(c.clone());
                                }
                                if n == "calibre:series_index" {
                                    series_number = Some(c);
                                }
                            }
                        },
                        _ => {}
                    }
                },
                Ok(Event::End(ref e)) => {
                    let name = e.local_name();
                    match name.as_ref() {
                        b"metadata" => in_metadata = false,
                        _ => {}
                    }
                },
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow!("Error parsing OPF: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        let cover_id = meta_cover_id;
        let info = EpubInfo {
            title: title.unwrap_or_else(|| "Unknown Title".to_string()),
            authors,
            description,
            publisher,
            language,
            identifiers,
            subjects,
            series,
            series_number,
            cover_data: None,
            cover_mime_type: None,
        };
        Ok((info, cover_id))
    }

    fn find_cover_path(opf_xml: &str, cover_id: &Option<String>) -> Result<(Option<String>, Option<String>)> {
        use quick_xml::Reader;
        use quick_xml::events::Event;
        
        let mut reader = Reader::from_str(opf_xml);
        reader.trim_text(true);
        let mut buf = Vec::new();
        
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                    if e.local_name().as_ref() == b"item" {
                        let mut id = None;
                        let mut href = None;
                        let mut media_type = None;
                        let mut properties = None;
                        
                        for attr in e.attributes().flatten() {
                            let key = attr.key.as_ref();
                            if key == b"id" {
                                id = Some(String::from_utf8_lossy(&attr.value).to_string());
                            } else if key == b"href" {
                                href = Some(String::from_utf8_lossy(&attr.value).to_string());
                            } else if key == b"media-type" {
                                media_type = Some(String::from_utf8_lossy(&attr.value).to_string());
                            } else if key == b"properties" {
                                properties = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                        
                        if let (Some(href), Some(media_type)) = (href, media_type) {
                            if media_type.starts_with("image/") {
                                // Check if this is the cover using EPUB 3.0 properties
                                if let Some(props) = &properties {
                                    if props.contains("cover-image") {
                                        return Ok((Some(href), Some(media_type)));
                                    }
                                }
                                
                                // Check if this matches the cover_id from meta tags (EPUB 2.0 style)
                                if let (Some(cover_id), Some(id)) = (cover_id, &id) {
                                    if id == cover_id {
                                        return Ok((Some(href), Some(media_type)));
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow!("Error parsing manifest for cover: {}", e)),
                _ => {}
            }
            buf.clear();
        }
        Ok((None, None))
    }
} 