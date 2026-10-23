---
name: check-similarity-gd
description: Detect duplicate GDScript code using AST-based similarity analysis. Use when working with .gd files and looking for code duplication, refactoring opportunities, or enforcing code quality.
argument-hint: "[path] [--threshold 0.85] [--print]"
allowed-tools: Bash(similarity-gd *) Bash(cargo run -p similarity-gd *) Read Grep Glob
paths: "**/*.gd"
---

# GDScript Code Similarity Detection

## What to do

Run `similarity-gd` on the target GDScript project to detect duplicate functions, then analyze the results and propose a refactoring plan.

If `similarity-gd` is not installed:

```bash
cargo install similarity-gd
```

## Step 1: Run similarity analysis

Run with the arguments provided, or use sensible defaults:

```bash
similarity-gd $ARGUMENTS
```

If no arguments are given, scan the current directory:

```bash
similarity-gd . --threshold 0.85 --min-lines 5
```

## Step 2: Analyze the results

Read the output and categorize duplicate pairs:

### High-priority refactoring targets
- **100% similarity**: Exact structural duplicates. Extract into a shared function with parameters.
- **95-100% similarity**: Near-identical. Usually differ by 1-2 identifiers or values. Parameterize the difference.
- **X/Y axis pairs** (e.g. `get_width` vs `get_height`, `offset_x` vs `offset_y`): Common in layout/graphics code. Abstract with an axis parameter or generic.

### Medium-priority
- **85-95% similarity**: Same algorithm with minor structural differences. Consider extracting the common pattern.
- **async/sync variants**: Same logic duplicated for async and sync versions. Consider a shared core function.
- **`_with_options` variants** (e.g. `parse_rule` vs `parse_rule_with_diagnostics`): Use optional parameters or builder pattern.

### Low-priority / acceptable
- **Signal handler boilerplate**: Many nearly identical `_on_*_pressed` / `_on_*_body_entered` callbacks. Consider one shared handler bound with `Callable.bind()`.
- **Small accessors** (< 5 lines, 85-90%): Short functions naturally look similar. Usually not worth refactoring.

## Step 3: Propose refactoring

For each high-priority pair, suggest a concrete refactoring:

1. Show the two duplicate functions
2. Propose the unified version
3. Explain what callers need to change

## Key CLI Options

| Option | Description |
|--------|-------------|
| `--threshold <0-1>` | Similarity threshold (default: 0.85) |
| `--min-lines <n>` | Skip functions shorter than n lines (default: 3) |
| `--min-tokens <n>` | Skip functions with fewer than n AST nodes |
| `--print` | Show actual code snippets in output |
| `--filter-function <name>` | Only show pairs matching function name |
| `--filter-function-body <text>` | Only show pairs whose body contains text |
| `--fail-on-duplicates` | Exit code 1 if duplicates found (CI use) |
| `--rename-cost <0-1>` | APTED rename cost (default: 0.3, lower = more tolerant of renames) |
| `--no-size-penalty` | Disable penalty for differently-sized functions |

## Effective Thresholds

- `0.95+`: Nearly identical (variable renames only)
- `0.85-0.95`: Same algorithm, minor differences
- `0.75-0.85`: Similar structure, different details
- `0.70-0.75`: Related logic, worth investigating

## Workflow tips

- Start with `--threshold 0.9` to find obvious duplicates, then lower to 0.85
- Use `--print` to see actual code for top pairs
- Use `--min-lines 5` to skip trivial accessors
- Use `--filter-function <name>` to focus on a specific function
- For CI: `similarity-gd . --threshold 0.95 --fail-on-duplicates --min-lines 5`

## Common GDScript refactoring patterns

### Parameterize near-identical functions
```gdscript
# Before: two functions
func spawn_wave(index: int) -> void: ...
func spawn_boss_wave(level: int) -> void: ...

# After: one function with a scene/config parameter
func spawn(scene: PackedScene, level: int) -> void: ...
```

### Share behaviour through inheritance
Move duplicated methods of sibling scripts into a common base script (`class_name BaseEnemy`) and `extends` it.
