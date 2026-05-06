use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::env;
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use cargo_metadata::{MetadataCommand, PackageId};
use clap::{Args, Parser, Subcommand};
use globset::Glob;
use serde::{Deserialize, Serialize};

fn main() -> Result<()> {
    let cli = Cli::parse_from(cargo_subcommand_args());

    match cli.command {
        Commands::Packages(args) => {
            let plan = plan(&args)?;
            for package in &plan.packages {
                println!("{package}");
            }
        }
        Commands::PackageArgs(args) => {
            let plan = plan(&args)?;
            println!("{}", plan.package_args);
        }
        Commands::NextestExpr(args) => {
            let plan = plan(&args)?;
            println!("{}", plan.nextest_expr);
        }
        Commands::Explain(args) => {
            let plan = plan(&args)?;
            print_explanation(&plan);
        }
        Commands::Plan(args) => {
            let plan = plan(&args)?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
        Commands::CiTasks(args) => {
            let plan = plan(&args.common)?;
            print_ci_tasks(&plan, args.stage.as_deref())?;
        }
        Commands::CiRun(args) => {
            let plan = plan(&args.common)?;
            run_ci_tasks(&plan, args.stage.as_deref(), args.dry_run)?;
        }
    }

    Ok(())
}

#[derive(Debug, Parser)]
#[command(name = "cargo-affect")]
#[command(about = "Plan affected Rust workspace package checks from git changes.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Print selected package names, one per line.
    Packages(CommonArgs),
    /// Print selected packages as `-p <name>` Cargo arguments.
    PackageArgs(CommonArgs),
    /// Print a cargo-nextest package filter expression.
    NextestExpr(CommonArgs),
    /// Print selected packages with selection reasons.
    Explain(CommonArgs),
    /// Print the full JSON plan.
    Plan(CommonArgs),
    /// Print selected CI task ids from an optional profile.
    #[command(name = "ci-tasks")]
    CiTasks(CiTaskArgs),
    /// Run selected CI task commands from an optional profile.
    #[command(name = "ci-run")]
    CiRun(CiRunArgs),
}

#[derive(Debug, Clone, Args)]
struct CommonArgs {
    /// Cargo workspace root or manifest path.
    #[arg(long, default_value = ".")]
    workspace: Utf8PathBuf,

    /// Git base ref used when --changed-file is not provided.
    #[arg(long, default_value = "origin/main")]
    base: String,

    /// Explicit changed file. Repeatable; bypasses git diff for tests/CI glue.
    #[arg(long = "changed-file")]
    changed_files: Vec<Utf8PathBuf>,

    /// Policy config path. Defaults to affect.toml in the workspace root.
    #[arg(long)]
    config: Option<Utf8PathBuf>,

    /// Restrict output to a named package set from affect.toml.
    #[arg(long = "set")]
    package_sets: Vec<String>,

    /// Platform policy to apply. Defaults to the current OS name.
    #[arg(long)]
    platform: Option<String>,

    /// Optional CI profile from affect.toml. Profiles can supply set/platform/backend/task rules.
    #[arg(long)]
    profile: Option<String>,
}

#[derive(Debug, Clone, Args)]
struct CiTaskArgs {
    #[command(flatten)]
    common: CommonArgs,

    /// Optional task stage filter, for example setup, build, or test.
    #[arg(long)]
    stage: Option<String>,
}

#[derive(Debug, Clone, Args)]
struct CiRunArgs {
    #[command(flatten)]
    common: CommonArgs,

    /// Optional task stage filter, for example setup, build, or test.
    #[arg(long)]
    stage: Option<String>,

    /// Print selected task commands without executing them.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
struct Plan {
    workspace_root: String,
    base: String,
    changed_files: Vec<String>,
    packages: Vec<String>,
    package_args: String,
    nextest_expr: String,
    select_all: bool,
    cache_dimensions: CacheDimensions,
    #[serde(skip_serializing_if = "Option::is_none")]
    ci: Option<CiPlan>,
    reasons: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
struct CacheDimensions {
    workspace: String,
    package_group: String,
}

#[derive(Debug, Clone, Serialize)]
struct CiPlan {
    profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache: Option<String>,
    tasks: Vec<PlannedTask>,
}

#[derive(Debug, Clone, Serialize)]
struct PlannedTask {
    id: String,
    stage: String,
    run: String,
    working_directory: String,
    reasons: Vec<String>,
}

#[derive(Debug)]
struct WorkspacePackage {
    id: PackageId,
    name: String,
    root: Utf8PathBuf,
}

#[derive(Debug, Default, Deserialize)]
struct PolicyConfig {
    #[serde(default)]
    global: Vec<String>,
    #[serde(default)]
    paths: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    platform: BTreeMap<String, PlatformPolicy>,
    #[serde(default)]
    sets: BTreeMap<String, PackageSetPolicy>,
    #[serde(default)]
    ci: CiConfig,
}

#[derive(Debug, Default, Deserialize)]
struct PlatformPolicy {
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PackageSetPolicy {
    #[serde(default)]
    include: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CiConfig {
    #[serde(default)]
    profiles: BTreeMap<String, CiProfilePolicy>,
}

#[derive(Debug, Default, Deserialize)]
struct CiProfilePolicy {
    set: Option<String>,
    #[serde(default)]
    sets: Vec<String>,
    platform: Option<String>,
    backend: Option<String>,
    cache: Option<String>,
    #[serde(default)]
    tasks: Vec<CiTaskPolicy>,
}

#[derive(Debug, Default, Deserialize)]
struct CiTaskPolicy {
    id: String,
    #[serde(default)]
    stage: String,
    run: String,
    working_directory: Option<String>,
    #[serde(default)]
    when: CiTaskWhenPolicy,
}

#[derive(Debug, Default, Deserialize)]
struct CiTaskWhenPolicy {
    #[serde(default)]
    packages: Vec<String>,
    #[serde(default)]
    paths: Vec<String>,
}

fn cargo_subcommand_args() -> Vec<String> {
    let mut args = env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "affect") {
        args.remove(1);
    }
    args
}

fn plan(args: &CommonArgs) -> Result<Plan> {
    let workspace_manifest = workspace_manifest_path(&args.workspace)?;
    let metadata = MetadataCommand::new()
        .manifest_path(&workspace_manifest)
        .exec()
        .with_context(|| format!("failed to read cargo metadata for {workspace_manifest}"))?;

    let workspace_root = canonicalize_dir(&metadata.workspace_root)?;
    let config = load_policy_config(args, &workspace_root)?;
    let profile = resolve_ci_profile(&config, args.profile.as_deref())?;
    let package_sets = effective_package_sets(args, profile);
    let platform = effective_platform(args, profile);
    let packages = workspace_packages(&metadata)?;
    let reverse_deps = reverse_workspace_dependencies(&metadata, &packages);
    let changed_files = changed_files(args, &workspace_root)?;
    let changed_relative_files = changed_files
        .iter()
        .map(|file| relative_path(file, &workspace_root))
        .collect::<Vec<_>>();

    let mut selected = BTreeSet::<String>::new();
    let mut reasons = BTreeMap::<String, Vec<String>>::new();
    let mut queue = VecDeque::<String>::new();
    let mut select_all = false;

    let packages_by_name = packages
        .iter()
        .map(|package| (package.name.clone(), package))
        .collect::<HashMap<_, _>>();

    for changed_file in &changed_files {
        let changed_relative = relative_path(changed_file, &workspace_root);

        if is_global_impact_path(changed_file)
            || matches_glob_patterns(&config.global, &changed_relative)?
        {
            select_all = true;
            add_global_reason(&packages, &mut reasons, changed_file);
            continue;
        }

        if let Some(mapped_packages) = mapped_policy_packages(&config, &changed_relative)? {
            for package_name in mapped_packages {
                if !packages_by_name.contains_key(package_name.as_str()) {
                    bail!("affect.toml maps {changed_relative} to unknown package {package_name}");
                }
                if selected.insert(package_name.clone()) {
                    queue.push_back(package_name.clone());
                }
                reasons
                    .entry(package_name)
                    .or_default()
                    .push(format!("path rule: {changed_relative}"));
            }
            continue;
        }

        match owning_package(changed_file, &packages) {
            Some(package) => {
                if selected.insert(package.name.clone()) {
                    queue.push_back(package.name.clone());
                }
                reasons
                    .entry(package.name.clone())
                    .or_default()
                    .push(format!(
                        "changed: {}",
                        relative_path(changed_file, &workspace_root)
                    ));
            }
            None => {
                select_all = true;
                add_global_reason(&packages, &mut reasons, changed_file);
            }
        }
    }

    if select_all {
        selected = packages
            .iter()
            .map(|package| package.name.clone())
            .collect();
    } else {
        while let Some(package_name) = queue.pop_front() {
            for dependent in reverse_deps.get(&package_name).into_iter().flatten() {
                if selected.insert(dependent.clone()) {
                    queue.push_back(dependent.clone());
                    if packages_by_name.contains_key(dependent) {
                        reasons
                            .entry(dependent.clone())
                            .or_default()
                            .push(format!("depends on {package_name}"));
                    }
                }
            }
        }
    }

    selected = apply_package_set_filters(selected, &config, &package_sets)?;
    selected = apply_platform_excludes(selected, &config, &platform)?;

    reasons.retain(|package, _| selected.contains(package));

    let package_group = selected.iter().cloned().collect::<Vec<_>>().join(",");
    let selected_packages = selected.into_iter().collect::<Vec<_>>();
    let package_args = selected_packages
        .iter()
        .map(|package| format!("-p {package}"))
        .collect::<Vec<_>>()
        .join(" ");
    let nextest_expr = selected_packages
        .iter()
        .map(|package| format!("package({package})"))
        .collect::<Vec<_>>()
        .join(" | ");
    let ci = plan_ci(
        args.profile.as_deref(),
        profile,
        &selected_packages,
        &changed_relative_files,
        &package_args,
        &nextest_expr,
    )?;

    Ok(Plan {
        workspace_root: workspace_root.to_string(),
        base: args.base.clone(),
        changed_files: changed_relative_files,
        packages: selected_packages,
        package_args,
        nextest_expr,
        select_all,
        cache_dimensions: CacheDimensions {
            workspace: workspace_root.to_string(),
            package_group,
        },
        ci,
        reasons,
    })
}

fn resolve_ci_profile<'a>(
    config: &'a PolicyConfig,
    profile_name: Option<&str>,
) -> Result<Option<&'a CiProfilePolicy>> {
    let Some(profile_name) = profile_name else {
        return Ok(None);
    };

    config
        .ci
        .profiles
        .get(profile_name)
        .map(Some)
        .ok_or_else(|| anyhow!("unknown CI profile {profile_name} in affect.toml"))
}

fn effective_package_sets(args: &CommonArgs, profile: Option<&CiProfilePolicy>) -> Vec<String> {
    let mut sets = Vec::new();
    if let Some(profile) = profile {
        if let Some(set) = &profile.set {
            sets.push(set.clone());
        }
        sets.extend(profile.sets.iter().cloned());
    }
    sets.extend(args.package_sets.iter().cloned());
    sets
}

fn effective_platform(args: &CommonArgs, profile: Option<&CiProfilePolicy>) -> String {
    args.platform
        .clone()
        .or_else(|| profile.and_then(|profile| profile.platform.clone()))
        .unwrap_or_else(|| env::consts::OS.to_string())
}

fn plan_ci(
    profile_name: Option<&str>,
    profile: Option<&CiProfilePolicy>,
    selected_packages: &[String],
    changed_relative_files: &[String],
    package_args: &str,
    nextest_expr: &str,
) -> Result<Option<CiPlan>> {
    let Some(profile_name) = profile_name else {
        return Ok(None);
    };
    let Some(profile) = profile else {
        return Ok(None);
    };

    let tasks = planned_tasks(
        profile,
        selected_packages,
        changed_relative_files,
        package_args,
        nextest_expr,
    )?;

    Ok(Some(CiPlan {
        profile: profile_name.to_string(),
        backend: profile.backend.clone(),
        cache: profile.cache.clone(),
        tasks,
    }))
}

fn planned_tasks(
    profile: &CiProfilePolicy,
    selected_packages: &[String],
    changed_relative_files: &[String],
    package_args: &str,
    nextest_expr: &str,
) -> Result<Vec<PlannedTask>> {
    if selected_packages.is_empty() {
        return Ok(Vec::new());
    }

    profile
        .tasks
        .iter()
        .filter_map(|task| {
            task_match_reasons(task, selected_packages, changed_relative_files)
                .map(|reasons| reasons.map(|reasons| (task, reasons)))
                .transpose()
        })
        .map(|matched| {
            let (task, reasons) = matched?;
            if task.id.trim().is_empty() {
                bail!("CI task in affect.toml is missing an id");
            }
            if task.run.trim().is_empty() {
                bail!("CI task {} is missing a run command", task.id);
            }

            Ok(PlannedTask {
                id: task.id.clone(),
                stage: task_stage(task),
                run: render_task_run(task, selected_packages, package_args, nextest_expr),
                working_directory: task
                    .working_directory
                    .clone()
                    .unwrap_or_else(|| ".".to_string()),
                reasons,
            })
        })
        .collect()
}

fn task_match_reasons(
    task: &CiTaskPolicy,
    selected_packages: &[String],
    changed_relative_files: &[String],
) -> Result<Option<Vec<String>>> {
    let has_package_conditions = !task.when.packages.is_empty();
    let has_path_conditions = !task.when.paths.is_empty();

    if !has_package_conditions && !has_path_conditions {
        return Ok(Some(vec!["always".to_string()]));
    }

    let mut reasons = Vec::new();
    for pattern in &task.when.packages {
        for package in selected_packages {
            if glob_matches(pattern, package)? {
                reasons.push(format!("package {package} matches {pattern}"));
                break;
            }
        }
    }

    for pattern in &task.when.paths {
        for path in changed_relative_files {
            if glob_matches(pattern, path)? {
                reasons.push(format!("path {path} matches {pattern}"));
                break;
            }
        }
    }

    if reasons.is_empty() {
        Ok(None)
    } else {
        Ok(Some(reasons))
    }
}

fn task_stage(task: &CiTaskPolicy) -> String {
    if task.stage.trim().is_empty() {
        "default".to_string()
    } else {
        task.stage.clone()
    }
}

fn render_task_run(
    task: &CiTaskPolicy,
    selected_packages: &[String],
    package_args: &str,
    nextest_expr: &str,
) -> String {
    task.run
        .replace("{{ package_args }}", package_args)
        .replace("{{ nextest_expr }}", nextest_expr)
        .replace("{{ packages }}", &selected_packages.join(" "))
}

fn workspace_manifest_path(workspace: &Utf8Path) -> Result<Utf8PathBuf> {
    let workspace = absolute_path(workspace)?;
    if workspace.as_std_path().is_dir() {
        Ok(workspace.join("Cargo.toml"))
    } else {
        Ok(workspace)
    }
}

fn load_policy_config(args: &CommonArgs, workspace_root: &Utf8Path) -> Result<PolicyConfig> {
    let config_path = match &args.config {
        Some(path) if path.is_absolute() => Some(normalize_path(path.clone())),
        Some(path) => Some(normalize_path(workspace_root.join(path))),
        None => {
            let path = workspace_root.join("affect.toml");
            path.as_std_path().exists().then_some(path)
        }
    };

    let Some(config_path) = config_path else {
        return Ok(PolicyConfig::default());
    };

    let contents = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read policy config {config_path}"))?;
    toml::from_str(&contents)
        .with_context(|| format!("failed to parse policy config {config_path}"))
}

fn workspace_packages(metadata: &cargo_metadata::Metadata) -> Result<Vec<WorkspacePackage>> {
    let workspace_members = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut packages = metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
        .map(|package| {
            let root = package
                .manifest_path
                .parent()
                .ok_or_else(|| anyhow!("package {} has no manifest parent", package.name))?;
            Ok(WorkspacePackage {
                id: package.id.clone(),
                name: package.name.to_string(),
                root: canonicalize_dir(root)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    packages.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(packages)
}

fn reverse_workspace_dependencies(
    metadata: &cargo_metadata::Metadata,
    packages: &[WorkspacePackage],
) -> HashMap<String, BTreeSet<String>> {
    let workspace_ids = packages
        .iter()
        .map(|package| (package.id.clone(), package.name.clone()))
        .collect::<HashMap<_, _>>();
    let mut reverse_deps = HashMap::<String, BTreeSet<String>>::new();

    let Some(resolve) = &metadata.resolve else {
        return reverse_deps;
    };

    for node in &resolve.nodes {
        let Some(dependent_name) = workspace_ids.get(&node.id) else {
            continue;
        };

        for dep in &node.deps {
            if let Some(dependency_name) = workspace_ids.get(&dep.pkg) {
                reverse_deps
                    .entry(dependency_name.clone())
                    .or_default()
                    .insert(dependent_name.clone());
            }
        }
    }

    reverse_deps
}

fn changed_files(args: &CommonArgs, workspace_root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let changed_files = if args.changed_files.is_empty() {
        git_changed_files(&args.base, workspace_root)?
    } else {
        args.changed_files.clone()
    };
    let git_root = git_root(workspace_root).ok();

    changed_files
        .into_iter()
        .map(|file| {
            if file.is_absolute() {
                Ok(normalize_path(file))
            } else {
                let workspace_relative = canonicalize_existing_path(workspace_root.join(&file));
                if let Some(git_root) = &git_root {
                    let repo_relative = canonicalize_existing_path(git_root.join(&file));
                    if repo_relative.starts_with(workspace_root) {
                        return Ok(repo_relative);
                    }
                }
                Ok(workspace_relative)
            }
        })
        .collect()
}

fn git_root(workspace_root: &Utf8Path) -> Result<Utf8PathBuf> {
    let output = ProcessCommand::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to find git root for {workspace_root}"))?;

    if !output.status.success() {
        bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let root = String::from_utf8(output.stdout)
        .context("git rev-parse output was not utf-8")?
        .trim()
        .to_string();
    canonicalize_dir(Utf8Path::new(&root))
}

fn canonicalize_dir(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize directory {path}"))?;
    Utf8PathBuf::from_path_buf(canonical)
        .map(normalize_path)
        .map_err(|path| anyhow!("path is not utf-8: {}", path.display()))
}

fn canonicalize_existing_path(path: Utf8PathBuf) -> Utf8PathBuf {
    std::fs::canonicalize(&path)
        .ok()
        .and_then(|path| Utf8PathBuf::from_path_buf(path).ok())
        .map(normalize_path)
        .unwrap_or_else(|| normalize_path(path))
}

fn git_changed_files(base: &str, workspace_root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let output = ProcessCommand::new("git")
        .args(["diff", "--name-only", base, "--"])
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to run git diff in {workspace_root}"))?;

    if !output.status.success() {
        bail!(
            "git diff failed for base {base}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    String::from_utf8(output.stdout)
        .context("git diff output was not utf-8")?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(Utf8PathBuf::from)
        .collect::<Vec<_>>()
        .pipe(Ok)
}

fn owning_package<'a>(
    changed_file: &Utf8Path,
    packages: &'a [WorkspacePackage],
) -> Option<&'a WorkspacePackage> {
    packages
        .iter()
        .filter(|package| changed_file.starts_with(&package.root))
        .max_by_key(|package| package.root.as_str().len())
}

fn is_global_impact_path(changed_file: &Utf8Path) -> bool {
    changed_file
        .file_name()
        .is_some_and(|file_name| file_name == "Cargo.lock")
        || changed_file
            .components()
            .any(|component| matches!(component, Utf8Component::Normal(".cargo" | ".github")))
}

fn mapped_policy_packages(
    config: &PolicyConfig,
    relative_path: &str,
) -> Result<Option<Vec<String>>> {
    let mut matched = false;
    let mut packages = BTreeSet::new();

    for (pattern, mapped_packages) in &config.paths {
        if glob_matches(pattern, relative_path)? {
            matched = true;
            packages.extend(mapped_packages.iter().cloned());
        }
    }

    Ok(matched.then(|| packages.into_iter().collect()))
}

fn apply_package_set_filters(
    selected: BTreeSet<String>,
    config: &PolicyConfig,
    package_sets: &[String],
) -> Result<BTreeSet<String>> {
    if package_sets.is_empty() {
        return Ok(selected);
    }

    let mut include_patterns = Vec::new();
    for set_name in package_sets {
        let Some(set) = config.sets.get(set_name) else {
            bail!("unknown package set {set_name} in --set");
        };
        include_patterns.extend(set.include.iter().cloned());
    }

    selected
        .into_iter()
        .filter_map(|package| {
            matches_glob_patterns(&include_patterns, &package)
                .map(|matches| matches.then_some(package))
                .transpose()
        })
        .collect()
}

fn apply_platform_excludes(
    selected: BTreeSet<String>,
    config: &PolicyConfig,
    platform: &str,
) -> Result<BTreeSet<String>> {
    let Some(platform_policy) = config.platform.get(platform) else {
        return Ok(selected);
    };

    selected
        .into_iter()
        .filter_map(|package| {
            matches_glob_patterns(&platform_policy.exclude, &package)
                .map(|excluded| (!excluded).then_some(package))
                .transpose()
        })
        .collect()
}

fn matches_glob_patterns(patterns: &[String], value: &str) -> Result<bool> {
    for pattern in patterns {
        if glob_matches(pattern, value)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn glob_matches(pattern: &str, value: &str) -> Result<bool> {
    Ok(Glob::new(pattern)
        .with_context(|| format!("invalid glob pattern {pattern:?}"))?
        .compile_matcher()
        .is_match(value))
}

fn add_global_reason(
    packages: &[WorkspacePackage],
    reasons: &mut BTreeMap<String, Vec<String>>,
    changed_file: &Utf8Path,
) {
    for package in packages {
        reasons
            .entry(package.name.clone())
            .or_default()
            .push(format!("global impact: {changed_file}"));
    }
}

fn absolute_path(path: &Utf8Path) -> Result<Utf8PathBuf> {
    if path.is_absolute() {
        Ok(normalize_path(path.to_path_buf()))
    } else {
        let cwd = Utf8PathBuf::from_path_buf(env::current_dir()?)
            .map_err(|path| anyhow!("current directory is not utf-8: {}", path.display()))?;
        Ok(normalize_path(cwd.join(path)))
    }
}

fn normalize_path(path: Utf8PathBuf) -> Utf8PathBuf {
    let mut normalized = Utf8PathBuf::new();
    for component in path.components() {
        match component {
            Utf8Component::Prefix(prefix) => normalized.push(prefix.as_str()),
            Utf8Component::RootDir => normalized.push(component.as_str()),
            Utf8Component::CurDir => {}
            Utf8Component::ParentDir => {
                normalized.pop();
            }
            Utf8Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn relative_path(path: &Utf8Path, root: &Utf8Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string()
}

fn print_explanation(plan: &Plan) {
    if plan.packages.is_empty() {
        println!("No affected packages.");
        return;
    }

    for package in &plan.packages {
        println!("{package}");
        for reason in plan.reasons.get(package).into_iter().flatten() {
            println!("  - {reason}");
        }
    }
}

fn print_ci_tasks(plan: &Plan, stage: Option<&str>) -> Result<()> {
    for task in selected_ci_tasks(plan, stage)? {
        println!("{}", task.id);
    }
    Ok(())
}

fn run_ci_tasks(plan: &Plan, stage: Option<&str>, dry_run: bool) -> Result<()> {
    let workspace_root = Utf8Path::new(&plan.workspace_root);
    let tasks = selected_ci_tasks(plan, stage)?;
    if tasks.is_empty() {
        println!("No CI tasks selected.");
        return Ok(());
    }

    for task in tasks {
        let working_directory = task_working_directory(workspace_root, task)?;
        if dry_run {
            println!(
                "{} [{}] ({})\n{}",
                task.id, task.stage, working_directory, task.run
            );
            continue;
        }

        println!("Running CI task {} [{}]", task.id, task.stage);
        let status = ProcessCommand::new(env::var("CARGO_AFFECT_SHELL").unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd".to_string()
            } else {
                "bash".to_string()
            }
        }));
        let mut command = status;
        if cfg!(windows) {
            command.args(["/C", &task.run]);
        } else {
            command.args(["-lc", &task.run]);
        }
        let status = command
            .current_dir(&working_directory)
            .status()
            .with_context(|| format!("failed to run CI task {}", task.id))?;

        if !status.success() {
            bail!("CI task {} failed with {status}", task.id);
        }
    }

    Ok(())
}

fn selected_ci_tasks<'a>(plan: &'a Plan, stage: Option<&str>) -> Result<Vec<&'a PlannedTask>> {
    let Some(ci) = &plan.ci else {
        if stage.is_some() {
            bail!("--stage requires --profile with [ci.profiles] config");
        }
        return Ok(Vec::new());
    };

    Ok(ci
        .tasks
        .iter()
        .filter(|task| stage.is_none_or(|stage| task.stage == stage))
        .collect())
}

fn task_working_directory(workspace_root: &Utf8Path, task: &PlannedTask) -> Result<Utf8PathBuf> {
    let path = Utf8Path::new(&task.working_directory);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };
    Ok(normalize_path(path))
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn direct_change_selects_package_and_dependents() {
        let workspace = TestWorkspace::new();
        let plan = plan(&args(&workspace, ["a/src/lib.rs"])).unwrap();

        assert_eq!(plan.packages, vec!["a", "b"]);
        assert!(!plan.select_all);
        assert_eq!(plan.reasons["a"], vec!["changed: a/src/lib.rs"]);
        assert_eq!(plan.reasons["b"], vec!["depends on a"]);
    }

    #[test]
    fn leaf_change_selects_only_leaf_package() {
        let workspace = TestWorkspace::new();
        let plan = plan(&args(&workspace, ["b/src/lib.rs"])).unwrap();

        assert_eq!(plan.packages, vec!["b"]);
        assert!(!plan.select_all);
    }

    #[test]
    fn unmapped_change_selects_all_packages() {
        let workspace = TestWorkspace::new();
        let plan = plan(&args(&workspace, ["README.md"])).unwrap();

        assert_eq!(plan.packages, vec!["a", "b", "c"]);
        assert!(plan.select_all);
        assert!(plan.reasons["a"][0].contains("global impact:"));
    }

    #[test]
    fn cargo_lock_selects_all_packages() {
        let workspace = TestWorkspace::new();
        let plan = plan(&args(&workspace, ["Cargo.lock"])).unwrap();

        assert_eq!(plan.packages, vec!["a", "b", "c"]);
        assert!(plan.select_all);
    }

    #[test]
    fn output_formats_are_stable() {
        let workspace = TestWorkspace::new();
        let plan = plan(&args(&workspace, ["a/src/lib.rs"])).unwrap();

        assert_eq!(plan.package_args, "-p a -p b");
        assert_eq!(plan.nextest_expr, "package(a) | package(b)");
        assert_eq!(plan.cache_dimensions.package_group, "a,b");
        serde_json::to_string(&plan).unwrap();
    }

    #[test]
    fn config_global_rule_selects_all_packages() {
        let workspace = TestWorkspace::new();
        workspace.write_config(
            r#"
global = ["schema/**"]
"#,
        );

        let plan = plan(&args(&workspace, ["schema/gpu.json"])).unwrap();

        assert_eq!(plan.packages, vec!["a", "b", "c"]);
        assert!(plan.select_all);
    }

    #[test]
    fn path_mapping_selects_package_and_dependents() {
        let workspace = TestWorkspace::new();
        workspace.write_config(
            r#"
[paths]
"schema/**" = ["a"]
"#,
        );

        let plan = plan(&args(&workspace, ["schema/gpu.json"])).unwrap();

        assert_eq!(plan.packages, vec!["a", "b"]);
        assert_eq!(plan.reasons["a"], vec!["path rule: schema/gpu.json"]);
        assert_eq!(plan.reasons["b"], vec!["depends on a"]);
    }

    #[test]
    fn empty_path_mapping_selects_no_packages() {
        let workspace = TestWorkspace::new();
        workspace.write_config(
            r#"
[paths]
"docs/**" = []
"#,
        );

        let plan = plan(&args(&workspace, ["docs/readme.md"])).unwrap();

        assert!(plan.packages.is_empty());
        assert_eq!(plan.package_args, "");
        assert_eq!(plan.nextest_expr, "");
        assert!(!plan.select_all);
    }

    #[test]
    fn platform_excludes_remove_matching_packages() {
        let workspace = TestWorkspace::new();
        workspace.write_config(
            r#"
[platform.macos]
exclude = ["b"]
"#,
        );
        let mut args = args(&workspace, ["a/src/lib.rs"]);
        args.platform = Some("macos".to_string());

        let plan = plan(&args).unwrap();

        assert_eq!(plan.packages, vec!["a"]);
        assert!(!plan.reasons.contains_key("b"));
    }

    #[test]
    fn package_set_filters_selection() {
        let workspace = TestWorkspace::new();
        workspace.write_config(
            r#"
[sets.core]
include = ["a"]
"#,
        );
        let mut args = args(&workspace, ["a/src/lib.rs"]);
        args.package_sets = vec!["core".to_string()];

        let plan = plan(&args).unwrap();

        assert_eq!(plan.packages, vec!["a"]);
        assert_eq!(plan.cache_dimensions.package_group, "a");
    }

    #[test]
    fn ci_profile_supplies_set_platform_and_tasks() {
        let workspace = TestWorkspace::new();
        workspace.write_config(
            r#"
[platform.linux]
exclude = ["b"]

[sets.core]
include = ["a", "b"]

[ci.profiles.core-linux]
set = "core"
platform = "linux"
backend = "warpbuild"
cache = "core-linux"

[[ci.profiles.core-linux.tasks]]
id = "build"
stage = "build"
run = "cargo build {{ package_args }}"

[[ci.profiles.core-linux.tasks]]
id = "nextest"
stage = "test"
run = "cargo nextest run -E '{{ nextest_expr }}'"
when.packages = ["a"]

[[ci.profiles.core-linux.tasks]]
id = "docs"
stage = "test"
run = "cargo test -p c"
when.paths = ["docs/**"]
"#,
        );
        let mut args = args(&workspace, ["a/src/lib.rs"]);
        args.profile = Some("core-linux".to_string());

        let plan = plan(&args).unwrap();

        assert_eq!(plan.packages, vec!["a"]);
        let ci = plan.ci.unwrap();
        assert_eq!(ci.profile, "core-linux");
        assert_eq!(ci.backend.as_deref(), Some("warpbuild"));
        assert_eq!(ci.cache.as_deref(), Some("core-linux"));
        assert_eq!(
            ci.tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["build", "nextest"]
        );
        assert_eq!(ci.tasks[0].run, "cargo build -p a");
        assert_eq!(ci.tasks[1].run, "cargo nextest run -E 'package(a)'");
    }

    #[test]
    fn ci_tasks_are_optional_without_profile() {
        let workspace = TestWorkspace::new();
        workspace.write_config(
            r#"
[sets.core]
include = ["a"]

[ci.profiles.core]
set = "core"
"#,
        );

        let plan = plan(&args(&workspace, ["a/src/lib.rs"])).unwrap();

        assert_eq!(plan.packages, vec!["a", "b"]);
        assert!(plan.ci.is_none());
    }

    #[test]
    fn repo_relative_paths_work_for_nested_workspace() {
        let repo = tempfile::tempdir().unwrap();
        let repo_root = Utf8PathBuf::from_path_buf(repo.path().to_path_buf()).unwrap();
        run_git(&repo_root, ["init"]);

        let workspace = TestWorkspace::new_at(repo_root.join("crates"));
        let plan = plan(&CommonArgs {
            workspace: workspace.root(),
            base: "origin/main".to_string(),
            changed_files: vec![Utf8PathBuf::from("crates/a/src/lib.rs")],
            config: None,
            package_sets: Vec::new(),
            platform: None,
            profile: None,
        })
        .unwrap();

        assert_eq!(plan.changed_files, vec!["a/src/lib.rs"]);
        assert_eq!(plan.packages, vec!["a", "b"]);
        assert!(!plan.select_all);
    }

    #[test]
    fn cargo_subcommand_invocation_is_accepted() {
        let cli = Cli::try_parse_from([
            "cargo-affect",
            "packages",
            "--workspace",
            ".",
            "--changed-file",
            "src/main.rs",
        ]);
        assert!(cli.is_ok());

        let mut args = vec![
            "cargo-affect".to_string(),
            "affect".to_string(),
            "packages".to_string(),
        ];
        if args.get(1).is_some_and(|arg| arg == "affect") {
            args.remove(1);
        }
        assert!(Cli::try_parse_from(args).is_ok());
    }

    struct TestWorkspace {
        _dir: TempDir,
        root: Utf8PathBuf,
    }

    impl TestWorkspace {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
            Self::build(dir, root)
        }

        fn new_at(root: Utf8PathBuf) -> Self {
            fs::create_dir_all(&root).unwrap();
            let dir = tempfile::tempdir().unwrap();
            Self::build(dir, root)
        }

        fn build(dir: TempDir, root: Utf8PathBuf) -> Self {
            write(
                root.join("Cargo.toml"),
                r#"
[workspace]
members = ["a", "b", "c"]
resolver = "2"
"#,
            );
            create_package(&root, "a", "");
            create_package(&root, "b", r#"a = { path = "../a" }"#);
            create_package(&root, "c", "");

            Self { _dir: dir, root }
        }

        fn root(&self) -> Utf8PathBuf {
            self.root.clone()
        }

        fn write_config(&self, contents: &str) {
            write(self.root.join("affect.toml"), contents);
        }
    }

    fn args<const N: usize>(workspace: &TestWorkspace, changed_files: [&str; N]) -> CommonArgs {
        CommonArgs {
            workspace: workspace.root(),
            base: "origin/main".to_string(),
            changed_files: changed_files.into_iter().map(Utf8PathBuf::from).collect(),
            config: None,
            package_sets: Vec::new(),
            platform: None,
            profile: None,
        }
    }

    fn create_package(root: &Utf8Path, name: &str, dependencies: &str) {
        let package_root = root.join(name);
        fs::create_dir_all(package_root.join("src")).unwrap();
        write(
            package_root.join("Cargo.toml"),
            format!(
                r#"
[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[dependencies]
{dependencies}
"#
            ),
        );
        write(
            package_root.join("src/lib.rs"),
            "pub fn value() -> u8 { 1 }\n",
        );
    }

    fn write(path: Utf8PathBuf, contents: impl AsRef<[u8]>) {
        fs::write(path, contents).unwrap();
    }

    fn run_git<const N: usize>(dir: &Utf8Path, args: [&str; N]) {
        let output = ProcessCommand::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
