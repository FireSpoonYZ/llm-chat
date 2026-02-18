"""Tool description fragments for system prompt composition."""

TOOL_DESCRIPTIONS = {
    "bash": (
        "**bash** - Execute shell commands in the workspace directory.\n"
        "  - Use for running programs, installing packages, git operations, "
        "and other terminal tasks.\n"
        "  - Output exceeding 50,000 characters will be truncated. "
        "For large outputs, redirect to a file and use read.\n"
        "  - Default timeout is 30 seconds. Set a higher `timeout` "
        "for long-running commands (e.g., installs, builds).\n"
        "  - Avoid long-running or interactive commands "
        "(servers, watchers, editors).\n"
        "  - All commands run with /workspace as the working directory."
    ),
    "read": (
        "**read** - Read file contents with line numbers.\n"
        "  - Returns content in 'line_number | content' format.\n"
        "  - Supports `offset` and `limit` parameters for large files.\n"
        "  - Image files (.png, .jpg, .jpeg, .gif, .webp) are returned as "
        "visual content for direct recognition.\n"
        "  - Video/audio files return a sandbox:// reference with file size.\n"
        "  - Can read multiple files in parallel for efficiency."
    ),
    "write": (
        "**write** - Create or overwrite a file with new content.\n"
        "  - Parent directories are created automatically.\n"
        "  - Replaces the entire file content. "
        "Prefer edit for targeted changes to existing files.\n"
        "  - All file paths must be within /workspace."
    ),
    "edit": (
        "**edit** - Find and replace text in files with precision.\n"
        "  - Performs exact string replacement: old_string → new_string.\n"
        "  - old_string must be unique in the file. If not unique, "
        "include more surrounding context to disambiguate, "
        "or use `replace_all=true` to replace every occurrence.\n"
        "  - Preserve exact indentation (tabs/spaces) from the original.\n"
        "  - All file paths must be within /workspace."
    ),
    "glob": (
        "**glob** - Find files matching a glob pattern.\n"
        "  - Supports patterns like '**/*.py', 'src/**/*.ts'.\n"
        "  - Brace expansion supported (e.g., '*.{py,txt}').\n"
        "  - Returns up to 1000 matching file paths sorted alphabetically.\n"
        "  - `path` is optional. Omit `path` to search from workspace root.\n"
        "  - For default behavior, omit `path` instead of passing null/undefined.\n"
        "  - Empty or whitespace-only `path` is treated as workspace root for compatibility.\n"
        "  - Do not pass the literal strings 'undefined' or 'null' as `path`.\n"
        "  - Use `path` only when you need to limit search to a subdirectory."
    ),
    "grep": (
        "**grep** - Search file contents using regex patterns.\n"
        "  - Returns matching lines in 'filepath:lineno:content' format.\n"
        "  - Supports full regex syntax (e.g., 'log.*Error').\n"
        "  - Optional `glob_filter` to limit which files are searched.\n"
        "  - Optional `context` parameter for lines before/after each match.\n"
        "  - Binary files are automatically skipped."
    ),
    "web_fetch": (
        "**web_fetch** - Fetch content from a URL.\n"
        "  - HTML is converted to plain text for readability.\n"
        "  - Read-only operation; does not modify any files.\n"
        "  - Optional `max_length` parameter (default 50,000 characters).\n"
        "  - Use for retrieving documentation, API responses, "
        "or web page content."
    ),
    "web_search": (
        "**web_search** - Search the web for up-to-date information.\n"
        "  - Returns relevant web page content for a given query.\n"
        "  - Use to find current information, news, documentation, "
        "or answers to recent-events questions.\n"
        "  - Supports `type` parameter: 'auto' (default), 'fast', 'deep'.\n"
        "  - Include today's date in queries when searching for "
        "time-sensitive information."
    ),
    "code_interpreter": (
        "**code_interpreter** - Execute Python or JavaScript code.\n"
        "  - Use for calculations, data processing, visualization, "
        "and quick code experiments.\n"
        "  - Code runs in the workspace directory and can create "
        "or modify files.\n"
        "  - Output is captured and returned as text (30s timeout).\n"
        "  - Newly created media files (images, charts, SVG) are "
        "automatically detected and displayed inline."
    ),
    "image_generation": (
        "**image_generation** - Generate or edit images using AI.\n"
        "  - Provide a detailed prompt describing the desired image.\n"
        "  - `size`: WxH format (e.g., '1024x1024', '1024x1536', '1536x1024'). "
        "Google provider auto-maps to the closest supported aspect ratio.\n"
        "  - `quality`: low, medium, high, auto (default).\n"
        "  - `n`: number of images to generate (1-4).\n"
        "  - Optional `reference_image`: path (relative to /workspace) to an "
        "existing image to use as a starting point. The prompt then describes "
        "how to modify it. Supported formats: PNG, JPEG, GIF, WebP.\n"
        "  - Returns sandbox:// URLs that display inline in chat.\n"
        "  - Available for OpenAI and Google providers."
    ),
    "task": (
        "**task** - Delegate a specialized subagent for broad codebase exploration.\n"
        "  - Parameters: `subagent_type`, `description`, `prompt`.\n"
        "  - Current supported `subagent_type`: `explore`.\n"
        "  - Use when the request needs deep or wide investigation across "
        "multiple modules, architecture tracing, or uncertain code ownership.\n"
        "  - Do NOT use for simple targeted lookups (known file, known symbol, "
        "or searches limited to a few files) where direct tools are faster.\n"
        "  - `description` should be a short 3-5 word summary.\n"
        "  - `prompt` should include all necessary context, scope, constraints, "
        "and expected output format for the subagent.\n"
        "  - The subagent returns a summarized report and structured trace; "
        "you must synthesize and present the final answer to the user."
    ),
}


def format_tool_descriptions(tool_names: list[str]) -> str:
    """Format tool descriptions for inclusion in the system prompt."""
    parts = ["# Available Tools\n"]
    for name in tool_names:
        desc = TOOL_DESCRIPTIONS.get(name, f"**{name}** - Tool")
        parts.append(f"- {desc}")
    return "\n".join(parts)
