//! user tsort — perform topological sort.
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{self, BufRead, BufReader};

pub fn run() -> i32 {
    let mut file = "-".to_string();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: tsort [FILE]\nWrite totally ordered list consistent with partial ordering in FILE.\n");
                return 0;
            }
            "--version" => {
                println!("tsort (user_utils) 0.1.0");
                return 0;
            }
            s if s.starts_with('-') && s != "-" => {
                eprintln!("tsort: invalid option -- '{s}'");
                return 1;
            }
            other => file = other.to_string(),
        }
    }
    let reader: Box<dyn BufRead> = if file == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        match File::open(&file) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(e) => {
                eprintln!("tsort: {file}: {e}");
                return 1;
            }
        }
    };
    let mut tokens = Vec::new();
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("tsort: {e}");
                return 1;
            }
        };
        tokens.extend(line.split_whitespace().map(|s| s.to_string()));
    }
    if tokens.len() % 2 != 0 {
        eprintln!("tsort: odd number of input tokens");
        return 1;
    }
    let mut nodes: HashSet<String> = HashSet::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut indeg: HashMap<String, usize> = HashMap::new();
    for pair in tokens.chunks(2) {
        let a = &pair[0];
        let b = &pair[1];
        nodes.insert(a.clone());
        nodes.insert(b.clone());
        indeg.entry(a.clone()).or_insert(0);
        indeg.entry(b.clone()).or_insert(0);
        if a != b {
            adj.entry(a.clone()).or_default().push(b.clone());
            *indeg.entry(b.clone()).or_insert(0) += 1;
        }
    }
    let mut q: VecDeque<String> = indeg
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(k, _)| k.clone())
        .collect();
    q.make_contiguous().sort();
    let mut out = Vec::new();
    while let Some(n) = q.pop_front() {
        out.push(n.clone());
        if let Some(nexts) = adj.get(&n) {
            let mut added = Vec::new();
            for m in nexts {
                if let Some(d) = indeg.get_mut(m) {
                    *d -= 1;
                    if *d == 0 {
                        added.push(m.clone());
                    }
                }
            }
            added.sort();
            for m in added {
                q.push_back(m);
            }
        }
    }
    if out.len() != nodes.len() {
        eprintln!("tsort: input contains a loop");
        return 1;
    }
    for n in out {
        println!("{n}");
    }
    0
}
