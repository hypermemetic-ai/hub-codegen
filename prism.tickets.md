# prism: Python Codebase Health Reporter

prism analyzes a Python project directory and prints a structured health report covering
lines of code, cyclomatic complexity, symbol inventory, import graph, and code smells.
It uses only Python stdlib (os, sys, re, ast, json, pathlib, argparse, dataclasses,
collections, textwrap, typing). Target: Python 3.9+.

All source files go under `/workspace/prism/`.
All test files go under `/workspace/prism/tests/`.

Create `/workspace/prism/__init__.py` (empty) and `/workspace/prism/tests/__init__.py`
(empty) as needed.

---

# T1: File Scanner [agent]

Create `/workspace/prism/scanner.py`.

Implement a module that walks a directory tree and collects Python source files.

## Interface

```python
@dataclass
class SourceFile:
    path: str          # absolute path
    rel_path: str      # path relative to project root
    source: str        # full file contents as a string
    size_bytes: int

def scan(root: str, exclude_dirs: list[str] | None = None) -> list[SourceFile]:
    """Walk root recursively, return all .py files.
    Default exclude_dirs: ['.git', '__pycache__', '.venv', 'venv', 'node_modules', '.tox']
    Skips unreadable files silently.
    """
```

Also export `DEFAULT_EXCLUDE_DIRS: list[str]`.

## Tests

Create `/workspace/prism/tests/test_scanner.py`. Use `tempfile.mkdtemp()` to create
a real temporary directory structure. Tests must cover:

- Finds .py files recursively
- Skips excluded directories
- Returns correct rel_path (relative to root)
- SourceFile.source contains the actual file content
- Returns empty list for an empty directory
- Handles a single file at root

Clean up temp dirs with `shutil.rmtree` in teardown.

---

# T2: Line Counter [agent]

Create `/workspace/prism/loc.py`.

Implement a module that counts lines of code in a Python source string.

## Interface

```python
@dataclass
class LocResult:
    total: int      # all lines including blank
    blank: int      # lines that are empty or whitespace-only
    comment: int    # lines whose first non-whitespace char is '#'
    code: int       # total - blank - comment

def count_loc(source: str) -> LocResult:
    """Count lines in a Python source string."""

def count_loc_multi(sources: list[str]) -> LocResult:
    """Sum LocResult across multiple source strings."""
```

## Tests

Create `/workspace/prism/tests/test_loc.py`. Cover:

- Empty string → all zeros
- Only blank lines
- Only comment lines (# comment)
- Only code lines
- Mixed source: verify code = total - blank - comment
- Inline comments on code lines count as code (not comment)
- Docstrings (triple-quoted) count as code lines
- count_loc_multi sums correctly

---

# T3: Cyclomatic Complexity [agent]

Create `/workspace/prism/complexity.py`.

Implement a module that computes cyclomatic complexity for every function and method
in a Python source string using the `ast` module.

Cyclomatic complexity = 1 + number of branching nodes in the function body.
Branching nodes: `If`, `For`, `While`, `ExceptHandler`, `With`, `Assert`,
`comprehension` (inside ListComp/SetComp/DictComp/GeneratorExp), `BoolOp` (each `and`/`or`).

## Interface

```python
@dataclass
class FunctionComplexity:
    name: str           # "MyClass.my_method" or "my_function"
    lineno: int
    complexity: int
    is_method: bool

@dataclass
class ComplexityReport:
    functions: list[FunctionComplexity]
    average: float      # 0.0 if no functions
    maximum: int        # 0 if no functions
    high_complexity: list[FunctionComplexity]   # complexity > 10

def analyze_complexity(source: str, filepath: str = "<unknown>") -> ComplexityReport:
    """Parse source with ast.parse, walk to find all functions and methods,
    compute complexity for each. Returns empty report for unparseable source
    (catch SyntaxError silently)."""
```

## Tests

Create `/workspace/prism/tests/test_complexity.py`. Cover:

- A function with no branches → complexity 1
- A function with one if → complexity 2
- A function with if/elif/else → complexity 3 (elif counts)
- A function with a for loop and an if inside → complexity 3
- A method inside a class: name is "ClassName.method_name"
- Empty source → empty report with average 0.0
- SyntaxError source → empty report (no exception raised)
- high_complexity only includes functions with complexity > 10

---

# T4: Symbol Extractor [agent]

Create `/workspace/prism/symbols.py`.

Implement a module that extracts a symbol inventory from Python source using `ast`.

## Interface

```python
@dataclass
class SymbolSummary:
    classes: int
    functions: int          # top-level functions only (not methods)
    methods: int            # methods inside classes
    async_functions: int    # async def at top level
    async_methods: int      # async def inside classes
    class_names: list[str]
    function_names: list[str]  # top-level function names

def extract_symbols(source: str) -> SymbolSummary:
    """Parse source with ast.parse, walk top-level nodes to count symbols.
    Returns zeroed SymbolSummary for unparseable source."""
```

## Tests

Create `/workspace/prism/tests/test_symbols.py`. Cover:

- Empty source → all zeros
- One class with two methods → classes=1, methods=2, functions=0
- Two top-level functions → functions=2
- Async def at top level → async_functions=1
- Async def inside class → async_methods=1
- class_names contains correct names
- function_names contains correct names
- Nested class (class inside class): only outer counts toward top-level classes
- SyntaxError → zeroed summary, no exception

---

# T5: Import Analyzer [agent]

Create `/workspace/prism/imports.py`.

Implement a module that analyzes import statements in Python source using `ast`.

## Interface

```python
@dataclass
class ImportInfo:
    module: str         # top-level module name, e.g. "os" not "os.path"
    full_name: str      # full import: "os.path", "collections.abc"
    is_stdlib: bool     # True if module is in a known stdlib set
    lineno: int

@dataclass
class ImportReport:
    imports: list[ImportInfo]
    unique_modules: list[str]       # sorted list of unique top-level module names
    stdlib_count: int
    third_party_count: int
    most_common: list[tuple[str, int]]  # top 5 (module, count) by usage

STDLIB_MODULES: set[str]   # a reasonably complete set of stdlib module names

def analyze_imports(source: str) -> ImportReport:
    """Parse source, extract all import and from-import statements.
    Returns empty report for unparseable source."""
```

Populate `STDLIB_MODULES` with at least: os, sys, re, ast, json, io, abc, math,
time, datetime, pathlib, collections, itertools, functools, typing, dataclasses,
contextlib, copy, enum, hashlib, hmac, logging, argparse, unittest, tempfile,
shutil, subprocess, threading, multiprocessing, socket, http, urllib, xml, csv,
sqlite3, struct, array, base64, binascii, codecs, configparser, difflib, email,
fnmatch, fractions, gc, glob, gzip, heapq, html, imaplib, inspect, io, ipaddress,
keyword, linecache, locale, mimetypes, numbers, operator, pickle, platform, pprint,
profile, queue, random, selectors, signal, smtplib, ssl, stat, statistics, string,
tarfile, telnetlib, textwrap, traceback, types, uuid, warnings, weakref, zipfile,
zlib, builtins, __future__.

## Tests

Create `/workspace/prism/tests/test_imports.py`. Cover:

- `import os` → is_stdlib=True, module="os", full_name="os"
- `import os.path` → module="os", full_name="os.path"
- `from collections import OrderedDict` → module="collections"
- `import requests` → is_stdlib=False
- unique_modules is sorted and deduplicated
- most_common returns up to 5 entries
- Empty source → empty report
- SyntaxError → empty report

---

# T6: Code Smell Detector [agent]

Create `/workspace/prism/smells.py`.

Implement a module that detects common code smells in Python source using `ast`
and line-by-line analysis.

## Interface

```python
@dataclass
class Smell:
    kind: str       # "todo", "long_function", "deep_nesting", "bare_except", "print_call"
    message: str    # human-readable description
    lineno: int
    severity: str   # "info", "warning", "error"

@dataclass
class SmellReport:
    smells: list[Smell]
    by_kind: dict[str, int]     # {kind: count}
    total: int

def detect_smells(source: str, filepath: str = "<unknown>") -> SmellReport:
    """Detect smells. Rules:
    - todo: any line with TODO, FIXME, HACK, or XXX (case-insensitive) in a comment
    - long_function: any function/method body > 50 lines (severity: warning)
    - deep_nesting: any function with ast node depth > 5 from function root (severity: warning)
    - bare_except: `except:` with no exception type (severity: error)
    - print_call: call to `print(...)` at module level or inside a function (severity: info)
    SyntaxError → return empty SmellReport.
    """
```

## Tests

Create `/workspace/prism/tests/test_smells.py`. Cover:

- A line `# TODO: fix this` → smell of kind "todo"
- A line `x = 1  # FIXME ugly` → smell of kind "todo"
- A bare `except:` clause → smell of kind "bare_except"
- A print() call → smell of kind "print_call"
- A function longer than 50 lines → smell of kind "long_function"
- No smells in clean code
- SyntaxError source → empty report
- by_kind counts match actual smells list
- total equals len(smells)

---

# T7: Report Formatter [agent]

Create `/workspace/prism/report.py`.

Implement a module that takes analysis results and produces a formatted terminal
report as a string.

## Interface

```python
@dataclass
class AnalysisBundle:
    root: str
    file_count: int
    loc: "LocResult"            # from loc.py (use duck-typed dict or Any)
    complexity: "ComplexityReport"
    symbols: "SymbolSummary"
    imports: "ImportReport"
    smells: "SmellReport"

def format_report(bundle: AnalysisBundle, width: int = 72) -> str:
    """Return a multi-line string suitable for printing to a terminal.

    The report must include sections for:
    - Header: project root, file count, analysis timestamp
    - Lines of Code: total, blank, comment, code percentages
    - Complexity: average, maximum, count of high-complexity functions,
                  list of top-5 worst functions (name, line, complexity)
    - Symbols: class count, function count, method count, async counts
    - Top Imports: top 5 most-used modules with stdlib vs third-party label
    - Code Smells: total count, breakdown by kind, list of error-severity smells

    Use only ASCII box-drawing characters: ─ │ ┌ ┐ └ ┘ ├ ┤
    (U+2500, U+2502, U+250C, U+2510, U+2514, U+2518, U+251C, U+2524)
    Wrap lines at `width` characters.
    """

def format_summary_line(bundle: AnalysisBundle) -> str:
    """Return a single-line summary:
    e.g. '12 files · 1,204 LOC · avg complexity 2.4 · 3 smells'
    """
```

The `AnalysisBundle` fields use duck typing — it only accesses attributes by name,
so the actual types from the other modules will work when wired up.

## Tests

Create `/workspace/prism/tests/test_report.py`. Use `SimpleNamespace` or dataclasses
to construct mock `AnalysisBundle` objects without importing other prism modules. Cover:

- `format_report` returns a non-empty string
- The string contains the root path
- The string contains LOC information
- `format_summary_line` returns a single line (no newlines)
- `format_summary_line` contains file count
- Report width doesn't exceed `width + 10` chars per line (allow slight overflow for content)

---

# SYNTH: CLI Entry Point and Project Wiring [agent/synthesize]

blocked_by: [T1, T2, T3, T4, T5, T6, T7]
validate: cd /workspace/prism && python -m pytest tests/ -q 2>&1 | tail -5

Create `/workspace/prism/__main__.py` and `/workspace/prism/cli.py`.

You have output from 7 parallel agents. Wire them into a working CLI.

## `cli.py`

```python
def run_analysis(root: str) -> AnalysisBundle:
    """
    1. scanner.scan(root) → list[SourceFile]
    2. For each file, run loc.count_loc, complexity.analyze_complexity,
       symbols.extract_symbols, imports.analyze_imports, smells.detect_smells
    3. Aggregate results:
       - loc: count_loc_multi across all sources
       - complexity: merge all FunctionComplexity lists, recompute average/max/high
       - symbols: sum all SymbolSummary fields, union class_names/function_names
       - imports: merge ImportReport (combine lists, recompute unique_modules,
                  stdlib_count, third_party_count, most_common)
       - smells: merge all SmellReport lists, recompute by_kind and total
    4. Return AnalysisBundle
    """

def main():
    """argparse CLI:

    usage: python -m prism [path] [--json] [--summary]

    path: directory to analyze (default: current directory)
    --json: output raw JSON instead of formatted report
    --summary: print only the one-line summary

    Exit code 0 on success, 1 on error.
    """
```

## `__main__.py`

```python
from prism.cli import main
if __name__ == "__main__":
    main()
```

Also ensure `/workspace/prism/__init__.py` exists (empty is fine).
And `/workspace/prism/tests/__init__.py` exists (empty is fine).

## Merging imports for most_common

Collect all ImportInfo across files. Count occurrences of each top-level module name.
Sort descending, take top 5. Recompute stdlib_count and third_party_count as totals
across all files (not unique — a module used in 3 files counts 3).

## Tests

Create `/workspace/prism/tests/test_cli.py`. Use `tempfile.mkdtemp()` to build a
small synthetic Python project (2–3 .py files with known content). Cover:

- `run_analysis(root)` returns an `AnalysisBundle`
- `bundle.file_count` equals the number of .py files created
- `bundle.loc.total > 0`
- `bundle.symbols.functions >= 0`
- The formatted report string contains the temp dir path
- `format_summary_line` returns a single line

Do NOT test that running `python -m prism` as a subprocess works — just test the
Python API via `run_analysis` and `format_report`.
