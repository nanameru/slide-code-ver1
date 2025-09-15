use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use nucleo_matcher::Matcher;
use nucleo_matcher::Utf32Str;
use nucleo_matcher::pattern::AtomKind;
use nucleo_matcher::pattern::CaseMatching;
use nucleo_matcher::pattern::Normalization;
use nucleo_matcher::pattern::Pattern;
use serde::Serialize;
use std::cell::UnsafeCell;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::num::NonZero;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::process::Command;

mod cli;

pub use cli::Cli;

#[derive(Debug, Clone, Serialize)]
pub struct FileMatch {
    pub score: u32,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indices: Option<Vec<u32>>,
}

pub struct FileSearchResults {
    pub matches: Vec<FileMatch>,
    pub total_match_count: usize,
}

pub trait Reporter {
    fn report_match(&self, file_match: &FileMatch);
    fn warn_matches_truncated(&self, total_match_count: usize, shown_match_count: usize);
    fn warn_no_search_pattern(&self, search_directory: &Path);
}

pub async fn run_main<T: Reporter>(
    Cli { pattern, limit, cwd, compute_indices, json: _, exclude, threads }: Cli,
    reporter: T,
) -> anyhow::Result<()> {
    let search_directory = match cwd { Some(dir) => dir, None => std::env::current_dir()? };
    let pattern_text = match pattern {
        Some(pattern) => pattern,
        None => {
            reporter.warn_no_search_pattern(&search_directory);
            #[cfg(unix)]
            Command::new("ls")
                .arg("-al")
                .current_dir(search_directory)
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .status()
                .await?;
            #[cfg(windows)]
            {
                Command::new("cmd")
                    .arg("/c")
                    .arg(search_directory)
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .await?;
            }
            return Ok(());
        }
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let FileSearchResults { total_match_count, matches } = run(
        &pattern_text,
        limit,
        &search_directory,
        exclude,
        threads,
        cancel_flag,
        compute_indices,
    )?;
    let match_count = matches.len();
    let matches_truncated = total_match_count > match_count;

    for file_match in matches { reporter.report_match(&file_match); }
    if matches_truncated { reporter.warn_matches_truncated(total_match_count, match_count); }

    Ok(())
}

pub fn run(
    pattern_text: &str,
    limit: NonZero<usize>,
    search_directory: &Path,
    exclude: Vec<String>,
    threads: NonZero<usize>,
    cancel_flag: Arc<AtomicBool>,
    compute_indices: bool,
) -> anyhow::Result<FileSearchResults> {
    let pattern = create_pattern(pattern_text);
    let WorkerCount { num_walk_builder_threads, num_best_matches_lists } = create_worker_count(threads);
    let best_matchers_per_worker: Vec<UnsafeCell<BestMatchesList>> = (0..num_best_matches_lists)
        .map(|_| UnsafeCell::new(BestMatchesList::new(limit.get(), pattern.clone(), Matcher::new(nucleo_matcher::Config::DEFAULT))))
        .collect();

    let mut walk_builder = WalkBuilder::new(search_directory);
    walk_builder.threads(num_walk_builder_threads);
    if !exclude.is_empty() {
        let mut override_builder = OverrideBuilder::new(search_directory);
        for exclude in exclude { override_builder.add(&format!("!{exclude}"))?; }
        let override_matcher = override_builder.build()?; walk_builder.overrides(override_matcher);
    }
    let walker = walk_builder.build_parallel();

    let index_counter = AtomicUsize::new(0);
    walker.run(|| {
        let index = index_counter.fetch_add(1, Ordering::Relaxed);
        let best_list_ptr = best_matchers_per_worker[index].get();
        let best_list = unsafe { &mut *best_list_ptr };

        const CHECK_INTERVAL: usize = 1024;
        let mut processed = 0;
        let cancel = cancel_flag.clone();

        Box::new(move |entry| {
            if let Some(path) = get_file_path(&entry, search_directory) { best_list.insert(path); }
            processed += 1;
            if processed % CHECK_INTERVAL == 0 && cancel.load(Ordering::Relaxed) {
                ignore::WalkState::Quit
            } else { ignore::WalkState::Continue }
        })
    });

    fn get_file_path<'a>(entry_result: &'a Result<ignore::DirEntry, ignore::Error>, search_directory: &std::path::Path,) -> Option<&'a str> {
        let entry = match entry_result { Ok(e) => e, Err(_) => return None };
        if entry.file_type().is_some_and(|ft| ft.is_dir()) { return None; }
        let path = entry.path(); match path.strip_prefix(search_directory) { Ok(rel_path) => rel_path.to_str(), Err(_) => None }
    }

    if cancel_flag.load(Ordering::Relaxed) { return Ok(FileSearchResults { matches: Vec::new(), total_match_count: 0 }); }

    let mut global_heap: BinaryHeap<Reverse<(u32, String)>> = BinaryHeap::new();
    let mut total_match_count = 0;
    for best_list_cell in best_matchers_per_worker.iter() {
        let best_list = unsafe { &*best_list_cell.get() };
        total_match_count += best_list.num_matches;
        for &Reverse((score, ref line)) in best_list.binary_heap.iter() {
            if global_heap.len() < limit.get() {
                global_heap.push(Reverse((score, line.clone())));
            } else if let Some(min_element) = global_heap.peek() {
                if score > min_element.0.0 {
                    global_heap.pop();
                    global_heap.push(Reverse((score, line.clone())));
                }
            }
        }
    }

    let mut raw_matches: Vec<(u32, String)> = global_heap.into_iter().map(|r| r.0).collect();
    sort_matches(&mut raw_matches);

    let mut matcher = if compute_indices { Some(Matcher::new(nucleo_matcher::Config::DEFAULT)) } else { None };
    let matches: Vec<FileMatch> = raw_matches.into_iter().map(|(score, path)| {
        let indices = if compute_indices {
            let mut buf = Vec::<char>::new();
            let haystack: Utf32Str<'_> = Utf32Str::new(&path, &mut buf);
            let mut idx_vec: Vec<u32> = Vec::new();
            if let Some(ref mut m) = matcher { pattern.indices(haystack, m, &mut idx_vec); }
            idx_vec.sort_unstable(); idx_vec.dedup(); Some(idx_vec)
        } else { None };
        FileMatch { score, path, indices }
    }).collect();

    Ok(FileSearchResults { matches, total_match_count })
}

fn sort_matches(matches: &mut [(u32, String)]) { matches.sort_by(|a, b| match b.0.cmp(&a.0) { std::cmp::Ordering::Equal => a.1.cmp(&b.1), other => other, }); }

struct BestMatchesList {
    max_count: usize,
    num_matches: usize,
    pattern: Pattern,
    matcher: Matcher,
    binary_heap: BinaryHeap<Reverse<(u32, String)>>,
    utf32buf: Vec<char>,
}

impl BestMatchesList {
    fn new(max_count: usize, pattern: Pattern, matcher: Matcher) -> Self {
        Self { max_count, num_matches: 0, pattern, matcher, binary_heap: BinaryHeap::new(), utf32buf: Vec::<char>::new() }
    }

    fn insert(&mut self, line: &str) {
        let haystack: Utf32Str<'_> = Utf32Str::new(line, &mut self.utf32buf);
        if let Some(score) = self.pattern.score(haystack, &mut self.matcher) {
            self.num_matches += 1;
            if self.binary_heap.len() < self.max_count {
                self.binary_heap.push(Reverse((score, line.to_string())));
            } else if let Some(min_element) = self.binary_heap.peek() {
                if score > min_element.0.0 {
                    self.binary_heap.pop();
                    self.binary_heap.push(Reverse((score, line.to_string())));
                }
            }
        }
    }
}

struct WorkerCount { num_walk_builder_threads: usize, num_best_matches_lists: usize }

fn create_worker_count(num_workers: NonZero<usize>) -> WorkerCount {
    let num_walk_builder_threads = num_workers.get();
    let num_best_matches_lists = num_walk_builder_threads + 1;
    WorkerCount { num_walk_builder_threads, num_best_matches_lists }
}

fn create_pattern(pattern: &str) -> Pattern { Pattern::new(pattern, CaseMatching::Smart, Normalization::Smart, AtomKind::Fuzzy) }


