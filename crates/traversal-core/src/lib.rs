use const_format::formatcp;
use grep::regex::RegexMatcher;
use grep::searcher::{BinaryDetection, SearcherBuilder, Sink};
use ignore::{WalkBuilder, WalkState};
use regex::Regex;
use std::collections::HashMap;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};

macro_rules! make_traverse_tag_regex {
    ($tag:expr) => {
        formatcp!(r"\[traverse-{TAG_NAME}:\s*(\S*)\s*\]", TAG_NAME = $tag)
    };
}

const TARGET_TAG_REGEX: &str = make_traverse_tag_regex!("tgt");
const LINK_TAG_REGEX: &str = make_traverse_tag_regex!("lnk");
const REGEX_STR: &str = formatcp!("{TARGET_TAG_REGEX}|{LINK_TAG_REGEX}");
static REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(REGEX_STR).expect("Failed to create regex"));

enum RegexGroup {
    Target = 1,
    Link = 2,
}

struct Aggregator<'a> {
    tag_list: &'a mut TagList,
    path: &'a Path,
}

impl<'a> Sink for Aggregator<'a> {
    type Error = io::Error;

    fn matched(
        &mut self,
        _searcher: &grep::searcher::Searcher,
        mat: &grep::searcher::SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        // TODO(mfeist): Do a manual byte search with help from memchr::memmem instead for a speed
        // up.
        let line_number = mat.line_number().unwrap_or(0);
        let bytes = mat.bytes();
        let line = std::str::from_utf8(bytes).unwrap_or("");

        if let Some(captures) = REGEX.captures(line) {
            if let Some(group) = captures.get(RegexGroup::Target as usize) {
                let tag_id = group.as_str().to_string();
                self.tag_list.target_tags.push(Tag {
                    id: tag_id,
                    file_path: Box::from(self.path),
                    line_number,
                    range: group.range(),
                })
            }
            if let Some(group) = captures.get(RegexGroup::Link as usize) {
                let tag_id = group.as_str().to_string();
                self.tag_list.link_tags.push(Tag {
                    id: tag_id,
                    file_path: Box::from(self.path),
                    line_number,
                    range: group.range(),
                })
            }
        }

        Ok(true)
    }
}

#[derive(Clone, Debug)]
pub struct Tag {
    pub id: String,
    pub file_path: Box<Path>,
    pub line_number: u64,
    pub range: Range<usize>,
}

pub struct TagList {
    pub target_tags: Vec<Tag>,
    pub link_tags: Vec<Tag>,
}

pub struct CombinedTagList {
    pub tag_lists: Vec<TagList>,
}

struct ThreadBuffer {
    tag_list: TagList,
    combined: Arc<RwLock<CombinedTagList>>,
}

impl Drop for ThreadBuffer {
    fn drop(&mut self) {
        let tag_list = TagList {
            target_tags: std::mem::take(&mut self.tag_list.target_tags),
            link_tags: std::mem::take(&mut self.tag_list.link_tags),
        };
        self.combined.write().unwrap().tag_lists.push(tag_list);
    }
}

type TagFindResult = Arc<RwLock<CombinedTagList>>;

pub fn find_tags(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> TagFindResult {
    let combined_tag_list = Arc::new(RwLock::new(CombinedTagList { tag_lists: vec![] }));

    let matcher = Arc::new(
        RegexMatcher::new_line_matcher(REGEX_STR).expect("Failed to create RegexMatcher."),
    );

    let mut paths_iter = paths.into_iter();
    let first_path = match paths_iter.next() {
        Some(path) => path,
        None => return combined_tag_list,
    };

    let mut walk_builder = WalkBuilder::new(first_path);
    for path in paths_iter {
        walk_builder.add(path);
    }

    let walker = walk_builder.build_parallel();

    // Iterate over all files in provided paths except ignored files
    walker.run(|| {
        let matcher_copy = Arc::clone(&matcher);
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(true)
            .build();
        let mut buffer = ThreadBuffer {
            tag_list: TagList {
                target_tags: Vec::new(),
                link_tags: Vec::new(),
            },
            combined: combined_tag_list.clone(),
        };
        Box::new(move |result| {
            let entry = match result {
                Ok(ent) => ent,
                Err(err) => {
                    eprintln!("Error walking directory: {}", err);
                    return WalkState::Continue;
                }
            };

            if !entry.file_type().unwrap().is_file() {
                return WalkState::Continue;
            }

            let aggregator = Aggregator {
                tag_list: &mut buffer.tag_list,
                path: entry.path(),
            };
            let _search_result =
                searcher.search_path(matcher_copy.as_ref(), entry.path(), aggregator);

            WalkState::Continue
        })
    });

    combined_tag_list
}

pub struct TagMapping {
    pub tags: Vec<Tag>,
    pub tag_indices_by_id: HashMap<String, Vec<usize>>,
    pub tag_indices_by_file: HashMap<PathBuf, Vec<usize>>,
}

impl TagMapping {
    fn new() -> TagMapping {
        TagMapping {
            tags: Vec::new(),
            tag_indices_by_id: HashMap::new(),
            tag_indices_by_file: HashMap::new(),
        }
    }

    fn add_tag(&mut self, tag: Tag) {
        let tag_index = self.tags.len();
        self.tag_indices_by_id
            .entry(tag.id.clone())
            .or_default()
            .push(tag_index);
        self.tag_indices_by_file
            .entry(tag.file_path.to_path_buf())
            .or_default()
            .push(tag_index);
        self.tags.push(tag);
    }
}

pub struct TagRegistry {
    pub target_tags: TagMapping,
    pub link_tags: TagMapping,
}

impl TagRegistry {
    pub fn new() -> TagRegistry {
        TagRegistry {
            target_tags: TagMapping::new(),
            link_tags: TagMapping::new(),
        }
    }
}

impl Default for TagRegistry {
    fn default() -> TagRegistry {
        TagRegistry::new()
    }
}

pub fn aggregate_tags(tags: TagFindResult) -> TagRegistry {
    let mut tag_registry = TagRegistry::new();

    for tag_list in tags.read().unwrap().tag_lists.iter() {
        for target_tag in tag_list.target_tags.iter() {
            tag_registry.target_tags.add_tag(target_tag.clone());
        }
        for link_tag in tag_list.link_tags.iter() {
            tag_registry.link_tags.add_tag(link_tag.clone());
        }
    }

    tag_registry
}
