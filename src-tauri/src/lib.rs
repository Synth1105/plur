use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use toml::Table;
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::ZipWriter;

const FIREFOX_EXTENSION_ID: &str = "plur@local";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ModConfig {
    repo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PlurConfig {
    #[serde(default)]
    mods: HashMap<String, ModConfig>,
}

#[derive(Debug, Serialize)]
struct ModInfo {
    name: String,
    repo: String,
    installed: bool,
}

#[derive(Debug, Serialize)]
struct BuildReport {
    logs: Vec<String>,
    result_path: String,
    result_js_path: String,
    result_chrome_path: String,
    theme_path: String,
    count: usize,
    js_count: usize,
    chrome_count: usize,
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn config_path() -> PathBuf {
    home_dir().join(".plur").join("config.toml")
}

fn mods_dir() -> PathBuf {
    home_dir().join(".plur").join("mods")
}

fn read_config() -> Result<PlurConfig, Box<dyn std::error::Error>> {
    let path = config_path();
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => String::new(),
    };

    let config = if content.trim().is_empty() {
        PlurConfig::default()
    } else {
        toml::from_str(&content)?
    };

    Ok(config)
}

fn write_config(config: &PlurConfig) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let toml_string = toml::to_string_pretty(config)?;
    fs::write(&path, toml_string)?;
    Ok(())
}

fn collect_mods(config: &PlurConfig) -> Vec<ModInfo> {
    let base_dir = mods_dir();
    let mut items: Vec<ModInfo> = config
        .mods
        .iter()
        .map(|(name, info)| {
            let installed = mod_target_dir(&base_dir, name, &info.repo).exists();
            ModInfo {
                name: name.to_string(),
                repo: info.repo.to_string(),
                installed,
            }
        })
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    items
}

fn log_line(logs: &mut Vec<String>, message: impl Into<String>) {
    logs.push(message.into());
}

fn import_legacy_mods(logs: &mut Vec<String>) -> Result<usize, String> {
    let scan = scan_legacy();
    if scan.repo_urls.is_empty() {
        return Ok(0);
    }

    let mut config = read_config().map_err(|e| e.to_string())?;
    let mut added = 0;

    for repo in scan.repo_urls {
        let name = repo_name_from_url(&repo);
        if name.is_empty() {
            continue;
        }
        match config.mods.get(name) {
            Some(existing) if existing.repo == repo => continue,
            Some(_) => continue,
            None => {
                config
                    .mods
                    .insert(name.to_string(), ModConfig { repo });
                added += 1;
            }
        }
    }

    if added > 0 {
        write_config(&config).map_err(|e| e.to_string())?;
        log_line(logs, format!("Imported {} legacy mods.", added));
    }

    Ok(added)
}

fn repo_name_from_url(repo_url: &str) -> &str {
    let trimmed = repo_url.trim_end_matches('/');
    let trimmed = trimmed.trim_end_matches(".git");
    trimmed.rsplit('/').next().unwrap_or("")
}

fn scan_legacy() -> LegacyScan {
    let mut scan = LegacyScan::default();
    let mut repo_set = HashSet::new();
    let mut css_set = HashSet::new();
    let mut js_set = HashSet::new();
    let mut chrome_css_set = HashSet::new();
    let mut theme_set = HashSet::new();

    for root in legacy_roots() {
        for json_path in find_candidate_json(&root) {
            if let Ok(contents) = fs::read_to_string(&json_path) {
                if let Ok(value) = serde_json::from_str::<Value>(&contents) {
                    extract_from_json(
                        &value,
                        json_path.parent(),
                        &mut repo_set,
                        &mut css_set,
                        &mut js_set,
                        &mut chrome_css_set,
                        &mut theme_set,
                    );
                }
            }
        }
    }

    scan.repo_urls = repo_set.into_iter().collect();
    scan.css_outputs = css_set.into_iter().collect();
    scan.js_outputs = js_set.into_iter().collect();
    scan.chrome_css_outputs = chrome_css_set.into_iter().collect();
    scan.theme_outputs = theme_set.into_iter().collect();
    scan
}

#[derive(Default)]
struct LegacyScan {
    repo_urls: Vec<String>,
    css_outputs: Vec<PathBuf>,
    js_outputs: Vec<PathBuf>,
    chrome_css_outputs: Vec<PathBuf>,
    theme_outputs: Vec<PathBuf>,
}

fn legacy_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let home = home_dir();

    roots.push(home.join(".sine"));
    roots.push(home.join(".config").join("sine"));
    roots.push(home.join(".config").join("cosmocreeper"));

    #[cfg(target_os = "macos")]
    {
        roots.push(
            home.join("Library")
                .join("Application Support")
                .join("Sine"),
        );
        roots.push(
            home.join("Library")
                .join("Application Support")
                .join("CosmoCreeper"),
        );
        roots.push(
            home.join("Library")
                .join("Application Support")
                .join("Firefox"),
        );
    }

    #[cfg(target_os = "linux")]
    {
        roots.push(home.join(".mozilla").join("firefox"));
        roots.push(home.join(".config").join("mozilla").join("firefox"));
        roots.push(
            home.join(".var")
                .join("app")
                .join("org.mozilla.firefox")
                .join(".mozilla")
                .join("firefox"),
        );
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            roots.push(PathBuf::from(appdata).join("Mozilla").join("Firefox"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("Sine"));
            roots.push(PathBuf::from(local).join("CosmoCreeper"));
        }
    }

    for profile in firefox_profile_dirs() {
        roots.push(profile.join("chrome"));
    }

    roots
}

fn find_candidate_json(root: &Path) -> Vec<PathBuf> {
    if !root.exists() {
        return Vec::new();
    }
    WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if !name.ends_with(".json") {
                return None;
            }
            if name.contains("sine")
                || name.contains("cosmo")
                || name.contains("mods")
                || name.contains("manager")
            {
                Some(e.path().to_path_buf())
            } else {
                None
            }
        })
        .collect()
}

fn extract_from_json(
    value: &Value,
    base_dir: Option<&Path>,
    repos: &mut HashSet<String>,
    css_outputs: &mut HashSet<PathBuf>,
    js_outputs: &mut HashSet<PathBuf>,
    chrome_css_outputs: &mut HashSet<PathBuf>,
    theme_outputs: &mut HashSet<PathBuf>,
) {
    match value {
        Value::String(s) => {
            if let Some(repo) = normalize_repo_string(s) {
                repos.insert(repo);
            }
            if let Some(path) = normalize_output_path(s, base_dir) {
                if path.extension().map(|e| e == "css").unwrap_or(false) {
                    if path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_lowercase().contains("chrome"))
                        .unwrap_or(false)
                    {
                        chrome_css_outputs.insert(path);
                    } else {
                        css_outputs.insert(path);
                    }
                } else if path.extension().map(|e| e == "js").unwrap_or(false) {
                    js_outputs.insert(path);
                } else if path.extension().map(|e| e == "json").unwrap_or(false) {
                    theme_outputs.insert(path);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                extract_from_json(
                    item,
                    base_dir,
                    repos,
                    css_outputs,
                    js_outputs,
                    chrome_css_outputs,
                    theme_outputs,
                );
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                let key_lc = key.to_lowercase();
                if (key_lc.contains("repo") || key_lc.contains("url")) && val.is_string() {
                    if let Some(repo) = val.as_str().and_then(normalize_repo_string) {
                        repos.insert(repo);
                    }
                }
                if (key_lc.contains("output") || key_lc.contains("path")) && val.is_string() {
                    if let Some(path) = val.as_str().and_then(|s| normalize_output_path(s, base_dir))
                    {
                        if path.extension().map(|e| e == "css").unwrap_or(false) {
                            if key_lc.contains("chrome") {
                                chrome_css_outputs.insert(path);
                            } else {
                                css_outputs.insert(path);
                            }
                        } else if path.extension().map(|e| e == "js").unwrap_or(false) {
                            js_outputs.insert(path);
                        } else if path.extension().map(|e| e == "json").unwrap_or(false) {
                            theme_outputs.insert(path);
                        }
                    }
                }
                extract_from_json(
                    val,
                    base_dir,
                    repos,
                    css_outputs,
                    js_outputs,
                    chrome_css_outputs,
                    theme_outputs,
                );
            }
        }
        _ => {}
    }
}

fn normalize_repo_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("github.com/") {
        let repo_part = trimmed
            .split("github.com/")
            .nth(1)?
            .trim_end_matches('/')
            .trim_end_matches(".git");
        if repo_part.contains('/') {
            return Some(format!("https://github.com/{}.git", repo_part));
        }
    }

    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() == 2 && is_repo_segment(parts[0]) && is_repo_segment(parts[1]) {
        return Some(format!("https://github.com/{}/{}.git", parts[0], parts[1]));
    }
    None
}

fn is_repo_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn normalize_output_path(value: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !(trimmed.ends_with(".css") || trimmed.ends_with(".js") || trimmed.ends_with(".json")) {
        return None;
    }

    let mut path = if trimmed.starts_with("~/") {
        home_dir().join(trimmed.trim_start_matches("~/"))
    } else if trimmed.starts_with('/') || trimmed.contains(":\\") || trimmed.contains(":/") {
        PathBuf::from(trimmed)
    } else if let Some(base) = base_dir {
        base.join(trimmed)
    } else {
        PathBuf::from(trimmed)
    };

    if path.is_relative() {
        if let Some(base) = base_dir {
            path = base.join(path);
        }
    }
    Some(path)
}

fn hash_suffix(input: &str) -> String {
    // Deterministic, low-collision suffix for directory names
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for b in input.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{:016x}", hash)
}

fn mod_target_dir(base_dir: &Path, mod_name: &str, repo: &str) -> PathBuf {
    let hash = hash_suffix(&format!("{}|{}", mod_name, repo));
    base_dir.join(format!("{}-{}", mod_name, hash))
}

fn dep_target_dir(mod_dir: &Path, dep_name: &str, dep_repo: &str) -> PathBuf {
    let hash = hash_suffix(&format!("{}|{}", dep_name, dep_repo));
    mod_dir.join("deps").join(format!("{}-{}", dep_name, hash))
}

fn process_module_deps(mod_dir: &Path, logs: &mut Vec<String>) {
    let plur_toml = mod_dir.join("plur.toml");

    if let Ok(plur_content) = fs::read_to_string(&plur_toml) {
        log_line(
            logs,
            format!("Found plur.toml in {}", mod_dir.display()),
        );

        match plur_content.parse::<Table>() {
            Ok(plur_config) => {
                if let Some(deps_table) = plur_config.get("deps").and_then(|v| v.as_table()) {
                    for (dep_name, dep_value) in deps_table {
                        log_line(logs, format!("Processing dependency {}...", dep_name));

                        if let Some(dep_table) = dep_value.as_table() {
                            if let Some(dep_repo) = dep_table.get("repo").and_then(|v| v.as_str()) {
                                let dep_target = dep_target_dir(mod_dir, dep_name, dep_repo);
                                if dep_target.exists() {
                                    log_line(
                                        logs,
                                        format!("Dependency {} already exists, skipping.", dep_name),
                                    );
                                    continue;
                                }

                                let output = Command::new("git")
                                    .arg("clone")
                                    .arg(dep_repo)
                                    .arg(&dep_target)
                                    .output();

                                match output {
                                    Ok(output) => {
                                        if output.status.success() {
                                            log_line(
                                                logs,
                                                format!("Dependency {} downloaded.", dep_name),
                                            );
                                        } else {
                                            log_line(
                                                logs,
                                                format!("Dependency {} download failed.", dep_name),
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        log_line(logs, format!("Git command failed: {}", e));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => log_line(logs, format!("plur.toml parse failed: {}", e)),
        }
    }
}

fn build_result(
    mods_path: &Path,
    logs: &mut Vec<String>,
) -> Result<(String, String, String, String, usize, usize, usize), String> {
    let output_path = shellexpand::tilde("~/.plur/result.css").to_string();
    let output_js_path = shellexpand::tilde("~/.plur/result.js").to_string();
    let output_chrome_path = shellexpand::tilde("~/.plur/result.chrome.css").to_string();
    let output_theme_path = shellexpand::tilde("~/.plur/theme.json").to_string();

    if let Some(parent) = Path::new(&output_path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut result_content = String::new();
    let mut result_js = String::new();
    let mut result_chrome = String::new();
    let mut theme_json: Option<Value> = None;
    let mut count = 0;
    let mut js_count = 0;
    let mut chrome_count = 0;
    let mut theme_count = 0;

    log_line(logs, format!("Searching for plur.css in {}", mods_path.display()));

    for entry in WalkDir::new(mods_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if path.file_name() == Some("plur.css".as_ref()) {
            let mut file = File::open(path).map_err(|e| e.to_string())?;
            let mut content = String::new();
            file.read_to_string(&mut content).map_err(|e| e.to_string())?;

            result_content.push_str(&format!("/* {} */\n", path.display()));
            result_content.push_str(&content);
            result_content.push_str("\n\n");

            log_line(logs, format!("Merged {}", path.display()));
            count += 1;
        } else if path.file_name() == Some("plur.js".as_ref()) {
            let mut file = File::open(path).map_err(|e| e.to_string())?;
            let mut content = String::new();
            file.read_to_string(&mut content).map_err(|e| e.to_string())?;

            result_js.push_str(&format!("// {}\n", path.display()));
            result_js.push_str(&content);
            result_js.push_str("\n\n");

            log_line(logs, format!("Merged JS {}", path.display()));
            js_count += 1;
        } else if path.file_name() == Some("plur.chrome.css".as_ref()) {
            let mut file = File::open(path).map_err(|e| e.to_string())?;
            let mut content = String::new();
            file.read_to_string(&mut content).map_err(|e| e.to_string())?;

            result_chrome.push_str(&format!("/* {} */\n", path.display()));
            result_chrome.push_str(&content);
            result_chrome.push_str("\n\n");

            log_line(logs, format!("Merged UI CSS {}", path.display()));
            chrome_count += 1;
        } else if path.file_name() == Some("plur.theme.json".as_ref()) {
            let mut file = File::open(path).map_err(|e| e.to_string())?;
            let mut content = String::new();
            file.read_to_string(&mut content).map_err(|e| e.to_string())?;
            match serde_json::from_str::<Value>(&content) {
                Ok(value) => {
                    theme_json = Some(value);
                    theme_count += 1;
                    log_line(logs, format!("Theme loaded {}", path.display()));
                }
                Err(e) => log_line(logs, format!("Theme parse failed: {}", e)),
            }
        }
    }

    let mut output_file = File::create(&output_path).map_err(|e| e.to_string())?;
    output_file
        .write_all(result_content.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut output_js_file = File::create(&output_js_path).map_err(|e| e.to_string())?;
    output_js_file
        .write_all(result_js.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut output_chrome_file = File::create(&output_chrome_path).map_err(|e| e.to_string())?;
    output_chrome_file
        .write_all(result_chrome.as_bytes())
        .map_err(|e| e.to_string())?;

    let theme_value = theme_json.unwrap_or_else(default_theme_json);
    let theme_string = serde_json::to_string_pretty(&theme_value).map_err(|e| e.to_string())?;
    let mut output_theme_file = File::create(&output_theme_path).map_err(|e| e.to_string())?;
    output_theme_file
        .write_all(theme_string.as_bytes())
        .map_err(|e| e.to_string())?;

    log_line(logs, format!("Wrote {}", output_path));
    log_line(logs, format!("Wrote {}", output_js_path));
    log_line(logs, format!("Wrote {}", output_chrome_path));
    log_line(logs, format!("Wrote {}", output_theme_path));
    if theme_count > 0 {
        log_line(logs, format!("Theme files merged: {}", theme_count));
    }

    Ok((
        output_path,
        output_js_path,
        output_chrome_path,
        output_theme_path,
        count,
        js_count,
        chrome_count,
    ))
}

#[tauri::command]
fn list_mods() -> Result<Vec<ModInfo>, String> {
    let config = read_config().map_err(|e| e.to_string())?;
    Ok(collect_mods(&config))
}

#[tauri::command]
fn add_mod(user: String, repo: String) -> Result<Vec<ModInfo>, String> {
    let mut config = read_config().map_err(|e| e.to_string())?;

    let repo_url = format!("https://github.com/{}/{}.git", user, repo);
    config
        .mods
        .insert(repo.clone(), ModConfig { repo: repo_url });

    write_config(&config).map_err(|e| e.to_string())?;
    Ok(collect_mods(&config))
}

#[tauri::command]
fn remove_mod(name: String, repo: String) -> Result<Vec<ModInfo>, String> {
    let mut config = read_config().map_err(|e| e.to_string())?;

    let removed = match config.mods.get(&name) {
        Some(mod_cfg) if mod_cfg.repo == repo => {
            config.mods.remove(&name);
            true
        }
        _ => false,
    };

    if removed {
        write_config(&config).map_err(|e| e.to_string())?;
        let base_dir = mods_dir();
        let target_dir = mod_target_dir(&base_dir, &name, &repo);
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir).map_err(|e| e.to_string())?;
        }
    }

    Ok(collect_mods(&config))
}

#[tauri::command]
fn sync_and_build() -> Result<BuildReport, String> {
    let mut logs = Vec::new();
    let _ = import_legacy_mods(&mut logs);

    let config = read_config().map_err(|e| e.to_string())?;
    let base_dir = mods_dir();
    fs::create_dir_all(&base_dir).map_err(|e| e.to_string())?;

    if config.mods.is_empty() {
        log_line(&mut logs, "No mods configured. Add a mod first.");
    }

    for (mod_name, mod_value) in config.mods {
        log_line(&mut logs, format!("Syncing {}...", mod_name));

        let target_dir = mod_target_dir(&base_dir, &mod_name, &mod_value.repo);
        if target_dir.exists() {
            log_line(
                &mut logs,
                format!("{} already exists, skipping clone.", mod_name),
            );
            process_module_deps(&target_dir, &mut logs);
            continue;
        }

        let output = Command::new("git")
            .arg("clone")
            .arg(&mod_value.repo)
            .arg(&target_dir)
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            log_line(&mut logs, format!("{} cloned.", mod_name));
            process_module_deps(&target_dir, &mut logs);
        } else {
            log_line(&mut logs, format!("{} clone failed.", mod_name));
        }
    }

    let (result_path, result_js_path, result_chrome_path, theme_path, count, js_count, chrome_count) =
        build_result(&base_dir, &mut logs)?;
    install_all(
        &result_path,
        &result_js_path,
        &result_chrome_path,
        &theme_path,
        &mut logs,
    )?;

    Ok(BuildReport {
        logs,
        result_path,
        result_js_path,
        result_chrome_path,
        theme_path,
        count,
        js_count,
        chrome_count,
    })
}

fn install_all(
    result_css: &str,
    result_js: &str,
    result_chrome_css: &str,
    theme_path: &str,
    logs: &mut Vec<String>,
) -> Result<(), String> {
    let css_path = Path::new(result_css);
    let js_path = Path::new(result_js);
    let chrome_css_path = Path::new(result_chrome_css);
    let theme_path = Path::new(theme_path);

    if !css_path.exists() && !js_path.exists() && !chrome_css_path.exists() && !theme_path.exists() {
        log_line(logs, "No result.css/result.js/result.chrome.css/theme.json found. Skipping install.");
        return Ok(());
    }

    install_firefox(css_path, js_path, chrome_css_path, logs).map_err(|e| e.to_string())?;
    install_firefox_policy(css_path, js_path, logs).map_err(|e| e.to_string())?;

    let chromium_targets = [
        "Google Chrome",
        "Chromium",
        "Microsoft Edge",
        "Brave Browser",
        "Vivaldi",
    ];
    for browser in chromium_targets {
        install_chromium(browser, css_path, js_path, theme_path, logs).map_err(|e| e.to_string())?;
    }

    copy_legacy_outputs(css_path, js_path, chrome_css_path, theme_path, logs);

    Ok(())
}

fn install_firefox(
    css_path: &Path,
    js_path: &Path,
    chrome_css_path: &Path,
    logs: &mut Vec<String>,
) -> std::io::Result<()> {
    let profiles = firefox_profile_dirs();
    if profiles.is_empty() {
        log_line(logs, "Firefox profiles not found. Skipping.");
        return Ok(());
    }

    for profile in profiles {
        let chrome_dir = profile.join("chrome");
        fs::create_dir_all(&chrome_dir)?;

        if css_path.exists() {
            let plur_css = chrome_dir.join("plur.userContent.css");
            fs::copy(css_path, &plur_css)?;
            ensure_usercontent_import(&chrome_dir, "plur.userContent.css")?;
            log_line(
                logs,
                format!("Firefox CSS installed in {}", chrome_dir.display()),
            );
        }

        if js_path.exists() {
            let plur_js = chrome_dir.join("plur.userContent.js");
            fs::copy(js_path, &plur_js)?;
            log_line(
                logs,
                format!(
                    "Firefox JS copied to {} (requires userChromeJS or loader)",
                    plur_js.display()
                ),
            );
        }

        if chrome_css_path.exists() {
            let plur_chrome = chrome_dir.join("plur.userChrome.css");
            fs::copy(chrome_css_path, &plur_chrome)?;
            ensure_userchrome_import(&chrome_dir, "plur.userChrome.css")?;
            log_line(
                logs,
                format!("Firefox UI CSS installed in {}", chrome_dir.display()),
            );
        }

        log_line(
            logs,
            "Firefox: ensure `toolkit.legacyUserProfileCustomizations.stylesheets` is enabled.",
        );
    }

    Ok(())
}

fn install_firefox_policy(
    css_path: &Path,
    js_path: &Path,
    logs: &mut Vec<String>,
) -> std::io::Result<()> {
    let ext_dir = home_dir().join(".plur").join("extension-firefox");
    let xpi_path = write_firefox_extension(&ext_dir, css_path, js_path)?;
    install_firefox_profile_extension(&xpi_path, logs)?;

    let mut wrote = 0;
    for policy_path in firefox_policy_paths() {
        if let Some(parent) = policy_path.parent() {
            if fs::create_dir_all(parent).is_err() && !parent.exists() {
                continue;
            }
        }

        let policy = firefox_policy_json(&xpi_path);
        if fs::write(&policy_path, policy).is_ok() {
            wrote += 1;
            log_line(
                logs,
                format!("Firefox policy written to {}", policy_path.display()),
            );
        }
    }

    if wrote == 0 {
        log_line(
            logs,
            "Firefox policy not written (missing permissions or install path).",
        );
    }

    Ok(())
}

fn install_firefox_profile_extension(
    xpi_path: &Path,
    logs: &mut Vec<String>,
) -> std::io::Result<()> {
    let profiles = firefox_profile_dirs();
    if profiles.is_empty() {
        return Ok(());
    }

    for profile in profiles {
        let ext_dir = profile.join("extensions");
        if let Err(e) = fs::create_dir_all(&ext_dir) {
            log_line(
                logs,
                format!("Firefox extensions dir create failed: {}", e),
            );
            continue;
        }
        let target = ext_dir.join(format!("{}.xpi", FIREFOX_EXTENSION_ID));
        if fs::copy(xpi_path, &target).is_ok() {
            log_line(
                logs,
                format!("Firefox extension updated in {}", target.display()),
            );
        }
    }

    Ok(())
}

fn firefox_policy_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from(
            "/Applications/Firefox.app/Contents/Resources/distribution/policies.json",
        ));
        paths.push(PathBuf::from(
            "/Applications/Firefox Developer Edition.app/Contents/Resources/distribution/policies.json",
        ));
    }

    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/etc/firefox/policies/policies.json"));
        paths.push(PathBuf::from("/usr/lib/firefox/distribution/policies.json"));
        paths.push(PathBuf::from("/usr/lib64/firefox/distribution/policies.json"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(program_files) = std::env::var("PROGRAMFILES") {
            paths.push(
                PathBuf::from(program_files)
                    .join("Mozilla Firefox")
                    .join("distribution")
                    .join("policies.json"),
            );
        }
        if let Ok(program_files) = std::env::var("PROGRAMFILES(X86)") {
            paths.push(
                PathBuf::from(program_files)
                    .join("Mozilla Firefox")
                    .join("distribution")
                    .join("policies.json"),
            );
        }
    }

    paths
}

fn firefox_policy_json(xpi_path: &Path) -> String {
    let url = path_to_file_url(xpi_path);
    format!(
        r#"{{
  "policies": {{
    "ExtensionSettings": {{
      "{ext_id}": {{
        "installation_mode": "force_installed",
        "install_url": "{url}"
      }}
    }},
    "Preferences": {{
      "toolkit.legacyUserProfileCustomizations.stylesheets": true
    }}
  }}
}}
"#,
        ext_id = FIREFOX_EXTENSION_ID,
        url = url
    )
}

fn path_to_file_url(path: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        let path = path
            .to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches("//")
            .to_string();
        return format!("file:///{}", path);
    }
    format!("file://{}", path.to_string_lossy())
}

fn ensure_usercontent_import(chrome_dir: &Path, filename: &str) -> std::io::Result<()> {
    let import_line = format!("@import \"{}\";", filename);
    let path = chrome_dir.join("userContent.css");
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    if !content.contains(&import_line) {
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&import_line);
        content.push('\n');
        fs::write(&path, content)?;
    }
    Ok(())
}

fn ensure_userchrome_import(chrome_dir: &Path, filename: &str) -> std::io::Result<()> {
    let import_line = format!("@import \"{}\";", filename);
    let path = chrome_dir.join("userChrome.css");
    let mut content = fs::read_to_string(&path).unwrap_or_default();
    if !content.contains(&import_line) {
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&import_line);
        content.push('\n');
        fs::write(&path, content)?;
    }
    Ok(())
}

fn firefox_profile_dirs() -> Vec<PathBuf> {
    let mut profiles = Vec::new();
    let home = home_dir();

    let mut roots = Vec::new();
    #[cfg(target_os = "macos")]
    {
        roots.push(home.join("Library").join("Application Support").join("Firefox"));
    }
    #[cfg(target_os = "linux")]
    {
        roots.push(home.join(".mozilla").join("firefox"));
        roots.push(home.join(".config").join("mozilla").join("firefox"));
        roots.push(
            home.join(".var")
                .join("app")
                .join("org.mozilla.firefox")
                .join(".mozilla")
                .join("firefox"),
        );
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            roots.push(PathBuf::from(appdata).join("Mozilla").join("Firefox"));
        }
    }

    for root in roots {
        if !root.exists() {
            continue;
        }

        let ini_path = root.join("profiles.ini");
        if ini_path.exists() {
            if let Ok(contents) = fs::read_to_string(&ini_path) {
                profiles.extend(parse_profiles_ini(&contents, &root));
            }
        }

        let profiles_dir = root.join("Profiles");
        if profiles_dir.exists() {
            if let Ok(entries) = fs::read_dir(&profiles_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        profiles.push(path);
                    }
                }
            }
        }
    }

    profiles.sort();
    profiles.dedup();
    profiles
}

fn parse_profiles_ini(contents: &str, base_dir: &Path) -> Vec<PathBuf> {
    let mut profiles = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_is_relative = true;

    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(path) = current_path.take() {
                profiles.push(resolve_profile_path(base_dir, &path, current_is_relative));
            }
            current_is_relative = true;
            continue;
        }

        if let Some(rest) = line.strip_prefix("Path=") {
            current_path = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("IsRelative=") {
            current_is_relative = rest.trim() == "1";
        }
    }

    if let Some(path) = current_path.take() {
        profiles.push(resolve_profile_path(base_dir, &path, current_is_relative));
    }

    profiles
}

fn resolve_profile_path(base_dir: &Path, path: &str, is_relative: bool) -> PathBuf {
    if is_relative {
        base_dir.join(path)
    } else {
        PathBuf::from(path)
    }
}

fn install_chromium(
    browser_name: &str,
    css_path: &Path,
    js_path: &Path,
    theme_path: &Path,
    logs: &mut Vec<String>,
) -> std::io::Result<()> {
    let ext_dir = home_dir().join(".plur").join("extension-chromium");
    write_chromium_extension(&ext_dir, css_path, js_path, theme_path)?;

    let home = home_dir();
    let mac_root = match browser_name {
        "Google Chrome" => home
            .join("Library")
            .join("Application Support")
            .join("Google")
            .join("Chrome"),
        "Chromium" => home
            .join("Library")
            .join("Application Support")
            .join("Chromium"),
        "Microsoft Edge" => home
            .join("Library")
            .join("Application Support")
            .join("Microsoft Edge"),
        "Brave Browser" => home
            .join("Library")
            .join("Application Support")
            .join("BraveSoftware")
            .join("Brave-Browser"),
        "Vivaldi" => home
            .join("Library")
            .join("Application Support")
            .join("Vivaldi"),
        _ => home
            .join("Library")
            .join("Application Support")
            .join(browser_name),
    };
    let linux_root = match browser_name {
        "Google Chrome" => home.join(".config").join("google-chrome"),
        "Chromium" => home.join(".config").join("chromium"),
        "Microsoft Edge" => home.join(".config").join("microsoft-edge"),
        "Brave Browser" => home.join(".config").join("BraveSoftware").join("Brave-Browser"),
        "Vivaldi" => home.join(".config").join("vivaldi"),
        _ => home.join(".config"),
    };

    let windows_root = match browser_name {
        "Google Chrome" => std::env::var("LOCALAPPDATA")
            .ok()
            .map(|v| PathBuf::from(v).join("Google").join("Chrome").join("User Data")),
        "Chromium" => std::env::var("LOCALAPPDATA")
            .ok()
            .map(|v| PathBuf::from(v).join("Chromium").join("User Data")),
        "Microsoft Edge" => std::env::var("LOCALAPPDATA")
            .ok()
            .map(|v| PathBuf::from(v).join("Microsoft").join("Edge").join("User Data")),
        "Brave Browser" => std::env::var("LOCALAPPDATA").ok().map(|v| {
            PathBuf::from(v)
                .join("BraveSoftware")
                .join("Brave-Browser")
                .join("User Data")
        }),
        "Vivaldi" => std::env::var("LOCALAPPDATA")
            .ok()
            .map(|v| PathBuf::from(v).join("Vivaldi").join("User Data")),
        _ => None,
    };

    if !mac_root.exists() && !linux_root.exists() && windows_root.as_ref().map(|p| p.exists()) != Some(true) {
        log_line(logs, format!("{} profile not found. Skipping.", browser_name));
        return Ok(());
    }

    log_line(
        logs,
        format!(
            "{} extension updated at {}",
            browser_name,
            ext_dir.display()
        ),
    );
    log_line(
        logs,
        format!(
            "Load unpacked extension in {} from {}",
            browser_name,
            ext_dir.display()
        ),
    );
    Ok(())
}

fn write_chromium_extension(
    ext_dir: &Path,
    css_path: &Path,
    js_path: &Path,
    theme_path: &Path,
) -> std::io::Result<()> {
    fs::create_dir_all(ext_dir)?;

    let manifest_path = ext_dir.join("manifest.json");
    let theme = load_theme_json(theme_path).unwrap_or_else(default_theme_json);
    let manifest = manifest_json_chromium(&theme);
    fs::write(&manifest_path, manifest)?;

    let target_css = ext_dir.join("result.css");
    let target_js = ext_dir.join("result.js");

    copy_or_empty(css_path, &target_css)?;
    copy_or_empty(js_path, &target_js)?;

    Ok(())
}

fn write_firefox_extension(
    ext_dir: &Path,
    css_path: &Path,
    js_path: &Path,
) -> std::io::Result<PathBuf> {
    fs::create_dir_all(ext_dir)?;

    let manifest_path = ext_dir.join("manifest.json");
    fs::write(&manifest_path, manifest_json_firefox())?;

    let target_css = ext_dir.join("result.css");
    let target_js = ext_dir.join("result.js");

    copy_or_empty(css_path, &target_css)?;
    copy_or_empty(js_path, &target_js)?;

    let xpi_path = ext_dir.join("plur-firefox.xpi");
    create_xpi(ext_dir, &xpi_path)?;

    Ok(xpi_path)
}

fn create_xpi(ext_dir: &Path, xpi_path: &Path) -> std::io::Result<()> {
    let file = fs::File::create(xpi_path)?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default();

    for name in ["manifest.json", "result.css", "result.js"] {
        let path = ext_dir.join(name);
        let data = fs::read(&path).unwrap_or_default();
        zip.start_file(name, options)?;
        zip.write_all(&data)?;
    }

    zip.finish()?;
    Ok(())
}

fn copy_or_empty(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.exists() {
        fs::copy(src, dst)?;
    } else {
        fs::write(dst, "")?;
    }
    Ok(())
}

fn manifest_json_chromium(theme: &serde_json::Value) -> String {
    let mut manifest = serde_json::json!({
        "manifest_version": 3,
        "name": "Plur Mod Injector",
        "version": "0.1.0",
        "description": "Injects plur result.css and result.js",
        "content_scripts": [
            {
                "matches": ["<all_urls>"],
                "css": ["result.css"],
                "js": ["result.js"],
                "run_at": "document_end"
            }
        ],
        "host_permissions": ["<all_urls>"]
    });
    if !theme.is_null() {
        manifest["theme"] = theme.clone();
    }
    serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_string())
}

fn manifest_json_firefox() -> String {
    format!(
        r#"{{
  "manifest_version": 2,
  "name": "Plur Mod Injector",
  "version": "0.1.0",
  "description": "Injects plur result.css and result.js",
  "applications": {{
    "gecko": {{
      "id": "{ext_id}"
    }}
  }},
  "content_scripts": [
    {{
      "matches": ["<all_urls>"],
      "css": ["result.css"],
      "js": ["result.js"],
      "run_at": "document_end"
    }}
  ]
}}
"#,
        ext_id = FIREFOX_EXTENSION_ID
    )
}

fn load_theme_json(path: &Path) -> Option<serde_json::Value> {
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn default_theme_json() -> serde_json::Value {
    serde_json::json!({
        "colors": {
            "frame": [28, 32, 38],
            "toolbar": [36, 41, 48],
            "tab_background_text": [235, 238, 241],
            "bookmark_text": [235, 238, 241],
            "toolbar_text": [235, 238, 241],
            "button_background": [54, 61, 70],
            "ntp_background": [246, 247, 249],
            "ntp_text": [32, 36, 42]
        }
    })
}

fn copy_legacy_outputs(
    css_path: &Path,
    js_path: &Path,
    chrome_css_path: &Path,
    theme_path: &Path,
    logs: &mut Vec<String>,
) {
    let scan = scan_legacy();
    let mut copied = 0;

    for output in scan.css_outputs {
        if let Some(parent) = output.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if css_path.exists() && fs::copy(css_path, &output).is_ok() {
            copied += 1;
            log_line(
                logs,
                format!("Legacy CSS output updated at {}", output.display()),
            );
        }
    }

    for output in scan.js_outputs {
        if let Some(parent) = output.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if js_path.exists() && fs::copy(js_path, &output).is_ok() {
            copied += 1;
            log_line(
                logs,
                format!("Legacy JS output updated at {}", output.display()),
            );
        }
    }

    for output in scan.chrome_css_outputs {
        if let Some(parent) = output.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if chrome_css_path.exists() && fs::copy(chrome_css_path, &output).is_ok() {
            copied += 1;
            log_line(
                logs,
                format!("Legacy UI CSS output updated at {}", output.display()),
            );
        }
    }

    for output in scan.theme_outputs {
        if let Some(parent) = output.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if theme_path.exists() && fs::copy(theme_path, &output).is_ok() {
            copied += 1;
            log_line(
                logs,
                format!("Legacy theme output updated at {}", output.display()),
            );
        }
    }

    if copied == 0 {
        log_line(logs, "No legacy outputs detected for copy.");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_mods,
            add_mod,
            remove_mod,
            sync_and_build
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_suffix_is_deterministic() {
        let a1 = hash_suffix("example");
        let a2 = hash_suffix("example");
        assert_eq!(a1, a2);
    }

    #[test]
    fn mod_target_dir_includes_hash() {
        let base = Path::new("mods");
        let dir = mod_target_dir(base, "alpha", "https://example.com/alpha.git");
        let name = dir.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("alpha-"));
    }

    #[test]
    fn dep_target_dir_includes_hash() {
        let base = Path::new("mods/alpha-123");
        let dir = dep_target_dir(base, "dep", "https://example.com/dep.git");
        let name = dir.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("dep-"));
    }

    #[test]
    fn manifest_includes_css_and_js() {
        let manifest = manifest_json_chromium(&default_theme_json());
        assert!(manifest.contains("result.css"));
        assert!(manifest.contains("result.js"));
        assert!(manifest.contains("manifest_version"));
    }

    #[test]
    fn firefox_manifest_includes_id() {
        let manifest = manifest_json_firefox();
        assert!(manifest.contains(FIREFOX_EXTENSION_ID));
        assert!(manifest.contains("\"manifest_version\": 2"));
    }

    #[test]
    fn parse_profiles_ini_handles_relative_and_absolute() {
        let ini = r#"
[Profile0]
Name=default
IsRelative=1
Path=Profiles/abcd.default

[Profile1]
Name=custom
IsRelative=0
Path=/tmp/custom.profile
"#;
        let base = Path::new("/home/test/.mozilla/firefox");
        let paths = parse_profiles_ini(ini, base);
        assert!(paths.contains(&base.join("Profiles/abcd.default")));
        assert!(paths.contains(&PathBuf::from("/tmp/custom.profile")));
    }
}
