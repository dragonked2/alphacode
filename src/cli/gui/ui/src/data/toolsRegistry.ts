// AlphaCode Tools Registry
// Comprehensive catalog of 51+ AlphaCode tools categorized into 5 domains:
// 'core', 'terminal', 'intelligence', 'web', 'security'

export type ToolCategory = 'core' | 'terminal' | 'intelligence' | 'web' | 'security';
export type ToolRiskLevel = 'safe' | 'caution' | 'critical';

export interface ToolParameter {
  name: string;
  type: string;
  required: boolean;
  desc: string;
  defaultValue?: string;
}

export interface ToolDefinition {
  id: string;
  name: string;
  category: ToolCategory;
  descriptionEn: string;
  descriptionAr: string;
  riskLevel: ToolRiskLevel;
  iconName: string;
  parameters: ToolParameter[];
  commandExample?: string;
}

export interface ToolCategoryInfo {
  id: ToolCategory;
  nameEn: string;
  nameAr: string;
  descriptionEn: string;
  descriptionAr: string;
  iconName: string;
}

export const TOOL_CATEGORIES: ToolCategoryInfo[] = [
  {
    id: 'core',
    nameEn: 'Core Filesystem',
    nameAr: 'نظام الملفات الأساسي',
    descriptionEn: 'High-performance file manipulation, multi-edits, unified diff patches, and code search.',
    descriptionAr: 'معالجة الملفات عالية الأداء، التعديلات المتعددة، رقع الفروقات الموحدة، وبحث الكود.',
    iconName: 'FolderTree',
  },
  {
    id: 'terminal',
    nameEn: 'Terminal & Execution',
    nameAr: 'الطرفية وبيئة التشغيل',
    descriptionEn: 'Shell command execution, Python runners, background processes, cron scheduler, and MCP client.',
    descriptionAr: 'تنفيذ أوامر الصدفة، تشغيل بايثون، إدارة العمليات الخلفية، الجدولة الزمنية، وبروتوكول MCP.',
    iconName: 'Terminal',
  },
  {
    id: 'intelligence',
    nameEn: 'Intelligence & Swarm',
    nameAr: 'الذكاء والوكلاء المتعددين',
    descriptionEn: 'Strategic planning, task checklists, long-term memory, session search, and swarm orchestration.',
    descriptionAr: 'التخطيط الاستراتيجي، متابعة المهام، الذاكرة الدائمة، البحث الدلالي، وتنسيق أسراب الوكلاء.',
    iconName: 'Brain',
  },
  {
    id: 'web',
    nameEn: 'Web & APIs',
    nameAr: 'الويب والواجهات البرمجية',
    descriptionEn: 'Live web search, scraper engines, headless browser automation, HTTP flow inspect, and JWT testing.',
    descriptionAr: 'البحث المباشر، محركات الكشط، أتمتة متصفح الويب، اعتراض تدفق HTTP، واختبار رموز JWT.',
    iconName: 'Globe',
  },
  {
    id: 'security',
    nameEn: 'Security & Recon',
    nameAr: 'الأمن والاستطلاع السيبراني',
    descriptionEn: 'Subdomain discovery, port scanning, web fuzzers, vulnerability scanners, SQLi, XSS, and CTF solvers.',
    descriptionAr: 'استكشاف النطاقات، مسح المنافذ، فاحصات الويب، اكتشاف الثغرات، حقن قواعد البيانات، وأدوات CTF.',
    iconName: 'ShieldAlert',
  },
];

export const TOOLS_DATABASE: ToolDefinition[] = [
  // ==========================================
  // A) CORE (9 Tools)
  // ==========================================
  {
    id: 'read',
    name: 'Read File',
    category: 'core',
    descriptionEn: 'Read contents of text, source code, or configuration files with optional line range.',
    descriptionAr: 'قراءة محتويات الملفات النصية والبرمجية والإعدادات مع تحديد نطاق الأسطر اختياريًا.',
    riskLevel: 'safe',
    iconName: 'FileText',
    commandExample: 'read(path: "src/main.rs", start_line: 1, end_line: 25)',
    parameters: [
      { name: 'path', type: 'string', required: true, desc: 'Absolute or relative file path to read' },
      { name: 'start_line', type: 'number', required: false, desc: 'Starting line number (1-indexed)' },
      { name: 'end_line', type: 'number', required: false, desc: 'Ending line number (inclusive)' },
    ],
  },
  {
    id: 'write',
    name: 'Write File',
    category: 'core',
    descriptionEn: 'Create a new file or completely overwrite an existing file with provided content.',
    descriptionAr: 'إنشاء ملف جديد أو استبدال محتوى ملف موجود بالكامل بالمحتوى المحدد.',
    riskLevel: 'caution',
    iconName: 'FilePlus',
    commandExample: 'write(path: "src/config.json", content: "{\\"version\\": \\"1.0\\"}")',
    parameters: [
      { name: 'path', type: 'string', required: true, desc: 'Destination file path' },
      { name: 'content', type: 'string', required: true, desc: 'Complete text content to write' },
      { name: 'overwrite', type: 'boolean', required: false, desc: 'Allow overwriting existing file', defaultValue: 'true' },
    ],
  },
  {
    id: 'edit',
    name: 'Edit File',
    category: 'core',
    descriptionEn: 'Perform exact contiguous string replacement inside an existing file.',
    descriptionAr: 'تعديل دقيق واستبدال مقاطع نصية مطابقة داخل ملف موجود.',
    riskLevel: 'caution',
    iconName: 'FileEdit',
    commandExample: 'edit(path: "src/server.ts", old_text: "const port = 3000;", new_text: "const port = 8080;")',
    parameters: [
      { name: 'path', type: 'string', required: true, desc: 'Target file path' },
      { name: 'old_text', type: 'string', required: true, desc: 'Exact character sequence to be replaced' },
      { name: 'new_text', type: 'string', required: true, desc: 'Replacement string' },
    ],
  },
  {
    id: 'multiedit',
    name: 'Multi-Edit File',
    category: 'core',
    descriptionEn: 'Apply multiple contiguous search and replace edits to a file in a single transactional batch.',
    descriptionAr: 'تطبيق عدة تعديلات متتالية واستبدالات على ملف في دفعة واحدة موثوقة.',
    riskLevel: 'caution',
    iconName: 'Layers',
    commandExample: 'multiedit(path: "src/App.tsx", edits: [{ old_text: "A", new_text: "B" }])',
    parameters: [
      { name: 'path', type: 'string', required: true, desc: 'Path of target file' },
      { name: 'edits', type: 'array', required: true, desc: 'Array of { old_text, new_text } objects to apply sequentially' },
    ],
  },
  {
    id: 'patch',
    name: 'Generate Patch',
    category: 'core',
    descriptionEn: 'Generate unified diff patch against a file or workspace tree.',
    descriptionAr: 'إنشاء رقعة فروقات موحدة (Unified Diff) لملف أو لشجرة مساحة العمل.',
    riskLevel: 'safe',
    iconName: 'GitPullRequest',
    commandExample: 'patch(path: "src/main.rs", context_lines: 3)',
    parameters: [
      { name: 'path', type: 'string', required: true, desc: 'Target file or directory path' },
      { name: 'context_lines', type: 'number', required: false, desc: 'Diff context lines around changes (default: 3)' },
    ],
  },
  {
    id: 'apply_patch',
    name: 'Apply Patch',
    category: 'core',
    descriptionEn: 'Apply a unified diff patch to files in the repository.',
    descriptionAr: 'تطبيق رقعة فروقات برمجية (Git Patch) على ملفات المستودع مباشرة.',
    riskLevel: 'caution',
    iconName: 'CheckCheck',
    commandExample: 'apply_patch(patch_content: "--- a/src/...", strip_count: 1)',
    parameters: [
      { name: 'patch_content', type: 'string', required: true, desc: 'Unified diff patch content' },
      { name: 'strip_count', type: 'number', required: false, desc: 'Prefix directory strip count (-p flag)' },
    ],
  },
  {
    id: 'ls',
    name: 'List Directory',
    category: 'core',
    descriptionEn: 'List files and directories with sizes, modification timestamps, and permissions.',
    descriptionAr: 'عرض قائمة الملفات والمجلدات مع الأحجام وتواريخ التعديل والصلاحيات.',
    riskLevel: 'safe',
    iconName: 'FolderTree',
    commandExample: 'ls(path: "src/components", depth: 2)',
    parameters: [
      { name: 'path', type: 'string', required: false, desc: 'Directory path (defaults to workspace root)' },
      { name: 'depth', type: 'number', required: false, desc: 'Recursion depth limit' },
    ],
  },
  {
    id: 'glob',
    name: 'Glob Search',
    category: 'core',
    descriptionEn: 'Fast pattern-based file search matching glob patterns across the workspace.',
    descriptionAr: 'البحث فائق السرعة عن الملفات باستخدام أنماط Glob عبر مساحة العمل.',
    riskLevel: 'safe',
    iconName: 'Search',
    commandExample: 'glob(pattern: "src/**/*.tsx", exclude: "node_modules/**")',
    parameters: [
      { name: 'pattern', type: 'string', required: true, desc: 'Glob pattern to match' },
      { name: 'exclude', type: 'string', required: false, desc: 'Glob pattern to ignore' },
    ],
  },
  {
    id: 'alphagrep',
    name: 'AlphaGrep',
    category: 'core',
    descriptionEn: 'Ripgrep-powered semantic and regex code search across the entire project.',
    descriptionAr: 'محرك بحث فائق السرعة بالكود باستخدام التعبيرات النمطية وRipgrep عبر كامل المشروع.',
    riskLevel: 'safe',
    iconName: 'FileSearch',
    commandExample: 'alphagrep(query: "pub fn init_tools", path: "src/")',
    parameters: [
      { name: 'query', type: 'string', required: true, desc: 'Regex pattern or search term' },
      { name: 'path', type: 'string', required: false, desc: 'Directory or file filter path' },
      { name: 'case_sensitive', type: 'boolean', required: false, desc: 'Match case sensitive' },
    ],
  },

  // ==========================================
  // B) TERMINAL (7 Tools)
  // ==========================================
  {
    id: 'bash',
    name: 'Bash / Shell Execution',
    category: 'terminal',
    descriptionEn: 'Execute commands in the terminal shell environment with captured stdout and stderr.',
    descriptionAr: 'تنفيذ أوامر الصدفة البرمجية والطرفية مع التقاط المخرجات والأخطاء.',
    riskLevel: 'critical',
    iconName: 'Terminal',
    commandExample: 'bash(command: "cargo check --workspace --tests")',
    parameters: [
      { name: 'command', type: 'string', required: true, desc: 'Command line string to execute' },
      { name: 'cwd', type: 'string', required: false, desc: 'Working directory path' },
      { name: 'timeout_ms', type: 'number', required: false, desc: 'Execution timeout in milliseconds' },
    ],
  },
  {
    id: 'python',
    name: 'Python Runner',
    category: 'terminal',
    descriptionEn: 'Run Python scripts or one-off code snippets in an isolated runtime.',
    descriptionAr: 'تشغيل نصوص بايثون البرمجية أو مقتطفات حسابية في بيئة تشغيلية آمنة.',
    riskLevel: 'caution',
    iconName: 'Code',
    commandExample: 'python(code: "import math; print(math.factorial(10))")',
    parameters: [
      { name: 'code', type: 'string', required: true, desc: 'Python code snippet to execute' },
      { name: 'args', type: 'array', required: false, desc: 'Command line arguments to pass' },
    ],
  },
  {
    id: 'bg',
    name: 'Background Process',
    category: 'terminal',
    descriptionEn: 'Manage, monitor, and query long-running background tasks and daemons.',
    descriptionAr: 'إدارة ومراقبة المهام والعمليات الخلفية الطويلة والخدمات التلقائية.',
    riskLevel: 'caution',
    iconName: 'Activity',
    commandExample: 'bg(action: "list")',
    parameters: [
      { name: 'action', type: 'string', required: true, desc: "'start' | 'stop' | 'status' | 'list' | 'logs'" },
      { name: 'task_id', type: 'string', required: false, desc: 'Background task ID' },
      { name: 'command', type: 'string', required: false, desc: 'Command for start action' },
    ],
  },
  {
    id: 'cron',
    name: 'Cron Scheduler',
    category: 'terminal',
    descriptionEn: 'Schedule recurring jobs or one-shot timers with cron expressions.',
    descriptionAr: 'جدولة المهام المتكررة أو المؤقتات المحددة بتعبيرات Cron الزمنية.',
    riskLevel: 'caution',
    iconName: 'Clock',
    commandExample: 'cron(schedule: "0 */4 * * *", command: "check_health")',
    parameters: [
      { name: 'schedule', type: 'string', required: true, desc: "Cron expression or duration in seconds" },
      { name: 'command', type: 'string', required: true, desc: 'Tool call or instruction to trigger' },
    ],
  },
  {
    id: 'open',
    name: 'System Open',
    category: 'terminal',
    descriptionEn: 'Open files, URLs, or folder paths in the default OS application or browser.',
    descriptionAr: 'فتح الملفات والروابط والمجلدات في التطبيق الافتراضي لنظام التشغيل.',
    riskLevel: 'safe',
    iconName: 'ExternalLink',
    commandExample: 'open(target: "http://localhost:5173")',
    parameters: [
      { name: 'target', type: 'string', required: true, desc: 'File path, URI, or HTTP URL to open' },
    ],
  },
  {
    id: 'clipboard',
    name: 'Clipboard Manager',
    category: 'terminal',
    descriptionEn: 'Read from or write text content to the OS system clipboard.',
    descriptionAr: 'قراءة النصوص أو نسخها مباشرة من وإلى الحافظة في نظام التشغيل.',
    riskLevel: 'safe',
    iconName: 'Clipboard',
    commandExample: 'clipboard(action: "write", text: "Bearer eyJhbGciOi...")',
    parameters: [
      { name: 'action', type: 'string', required: true, desc: "'read' | 'write'" },
      { name: 'text', type: 'string', required: false, desc: 'Text to copy to clipboard' },
    ],
  },
  {
    id: 'mcp',
    name: 'MCP Client',
    category: 'terminal',
    descriptionEn: 'Interact with Model Context Protocol servers, query tools, resources, and prompts.',
    descriptionAr: 'التواصل مع خوادم بروتوكول سياق النماذج (MCP) واستدعاء الأدوات والموارد.',
    riskLevel: 'caution',
    iconName: 'Cpu',
    commandExample: 'mcp(server_name: "filesystem", action: "list_tools")',
    parameters: [
      { name: 'server_name', type: 'string', required: true, desc: 'Registered MCP server name' },
      { name: 'action', type: 'string', required: true, desc: "'list_tools' | 'call_tool' | 'read_resource'" },
      { name: 'payload', type: 'object', required: false, desc: 'Arguments payload for tool call' },
    ],
  },

  // ==========================================
  // C) INTELLIGENCE (9 Tools)
  // ==========================================
  {
    id: 'plan',
    name: 'Planning Engine',
    category: 'intelligence',
    descriptionEn: 'Formulate, structure, and refine hierarchical multi-stage execution plans.',
    descriptionAr: 'صياغة وهيكلة خطط العمل الاستراتيجية متعددة المراحل وتحديثها.',
    riskLevel: 'safe',
    iconName: 'Compass',
    commandExample: 'plan(action: "create", steps: ["Scaffold components", "Verify build"])',
    parameters: [
      { name: 'action', type: 'string', required: true, desc: "'create' | 'update' | 'view'" },
      { name: 'steps', type: 'array', required: false, desc: 'Hierarchical execution milestones' },
    ],
  },
  {
    id: 'todo',
    name: 'Task Checklist',
    category: 'intelligence',
    descriptionEn: 'Track task items, mark milestones as in-progress or done, and maintain focus.',
    descriptionAr: 'متابعة قائمة المهام اللحظية، وتحديث حالة الإنجاز، والتركيز على الأولويات.',
    riskLevel: 'safe',
    iconName: 'CheckSquare',
    commandExample: 'todo(action: "update", id: "t2", status: "done")',
    parameters: [
      { name: 'action', type: 'string', required: true, desc: "'add' | 'update' | 'list' | 'remove'" },
      { name: 'id', type: 'string', required: false, desc: 'Task identifier' },
      { name: 'status', type: 'string', required: false, desc: "'pending' | 'in_progress' | 'done'" },
    ],
  },
  {
    id: 'memory',
    name: 'Persistent Memory',
    category: 'intelligence',
    descriptionEn: 'Store and retrieve cross-session memories, user preferences, and project insights.',
    descriptionAr: 'حفظ واسترجاع الذاكرة الدائمة عبر الجلسات وتفضيلات المستخدم ومعارف المشروع.',
    riskLevel: 'safe',
    iconName: 'Brain',
    commandExample: 'memory(action: "remember", key: "auth_type", value: "JWT in Bearer header")',
    parameters: [
      { name: 'action', type: 'string', required: true, desc: "'recall' | 'remember' | 'forget'" },
      { name: 'key', type: 'string', required: true, desc: 'Memory slot identifier' },
      { name: 'value', type: 'string', required: false, desc: 'Value or factual statement to memorize' },
    ],
  },
  {
    id: 'session_search',
    name: 'Session Search',
    category: 'intelligence',
    descriptionEn: 'Semantic vector search across past chat conversations, thoughts, and outputs.',
    descriptionAr: 'البحث الدلالي المتجهي عبر سجل المحادثات السابقة والأفكار والنتائج.',
    riskLevel: 'safe',
    iconName: 'History',
    commandExample: 'session_search(query: "vulnerability in subfinder output", limit: 3)',
    parameters: [
      { name: 'query', type: 'string', required: true, desc: 'Natural language search query' },
      { name: 'limit', type: 'number', required: false, desc: 'Maximum results to retrieve', defaultValue: '5' },
    ],
  },
  {
    id: 'skill',
    name: 'Agent Skill Loader',
    category: 'intelligence',
    descriptionEn: 'Load specialized agent skill instructions, templates, and domain guidelines.',
    descriptionAr: 'تحميل مهارات الوكيل المتخصصة والقوالب والإرشادات التخصصية.',
    riskLevel: 'safe',
    iconName: 'Sparkles',
    commandExample: 'skill(skill_name: "api-security")',
    parameters: [
      { name: 'skill_name', type: 'string', required: true, desc: 'Skill directory name or identifier' },
    ],
  },
  {
    id: 'tool_help',
    name: 'Tool Documentation',
    category: 'intelligence',
    descriptionEn: 'Inspect comprehensive schema, parameter types, and usage examples for any tool.',
    descriptionAr: 'فحص التوثيق الشامل ومخطط المعاملات وأمثلة الاستخدام لأي أداة في النظام.',
    riskLevel: 'safe',
    iconName: 'HelpCircle',
    commandExample: 'tool_help(tool_id: "nuclei")',
    parameters: [
      { name: 'tool_id', type: 'string', required: true, desc: 'Tool identifier to query' },
    ],
  },
  {
    id: 'self_improve',
    name: 'Self Optimization',
    category: 'intelligence',
    descriptionEn: 'Analyze interaction history and feedback to optimize system prompts and tool routing.',
    descriptionAr: 'تحليل أداء النموذج والملاحظات لتحسين صياغة التوجيهات واختيار الأدوات.',
    riskLevel: 'safe',
    iconName: 'TrendingUp',
    commandExample: 'self_improve(target_skill: "code_generation")',
    parameters: [
      { name: 'target_skill', type: 'string', required: false, desc: 'Target domain or workflow to analyze' },
    ],
  },
  {
    id: 'swarm',
    name: 'Swarm Orchestrator',
    category: 'intelligence',
    descriptionEn: 'Spawn, coordinate, message, and monitor multi-agent swarms working cooperatively.',
    descriptionAr: 'إطلاق وتوجيه ومراقبة أسراب الوكلاء المتعددين والتعاون التشاركي بينهم.',
    riskLevel: 'caution',
    iconName: 'Users',
    commandExample: 'swarm(action: "status")',
    parameters: [
      { name: 'action', type: 'string', required: true, desc: "'status' | 'add' | 'post' | 'kill' | 'model'" },
      { name: 'name', type: 'string', required: false, desc: 'Agent roster name' },
      { name: 'brief', type: 'string', required: false, desc: 'Brief assignment text' },
    ],
  },
  {
    id: 'batch',
    name: 'Batch Runner',
    category: 'intelligence',
    descriptionEn: 'Execute a batch of interdependent or parallel tool invocations transactionally.',
    descriptionAr: 'تنفيذ دفعة من استدعاءات الأدوات المتوازية أو المتتالية بصورة تكاملية.',
    riskLevel: 'caution',
    iconName: 'FastForward',
    commandExample: 'batch(calls: [{ name: "read", args: { ... } }, { name: "edit", args: { ... } }])',
    parameters: [
      { name: 'calls', type: 'array', required: true, desc: 'List of tool call objects to execute' },
      { name: 'stop_on_error', type: 'boolean', required: false, desc: 'Abort remaining calls if an error occurs', defaultValue: 'true' },
    ],
  },

  // ==========================================
  // D) WEB (6 Tools)
  // ==========================================
  {
    id: 'webfetch',
    name: 'Web Fetch',
    category: 'web',
    descriptionEn: 'Fetch web page content, parse clean HTML/text, and extract metadata.',
    descriptionAr: 'جلب محتوى صفحات الويب، واستخراج النصوص النقية والبيانات الوصفية.',
    riskLevel: 'safe',
    iconName: 'Globe',
    commandExample: 'webfetch(url: "https://docs.rs/tokio/latest/tokio/")',
    parameters: [
      { name: 'url', type: 'string', required: true, desc: 'HTTP or HTTPS target URL' },
      { name: 'raw_html', type: 'boolean', required: false, desc: 'Return unparsed HTML', defaultValue: 'false' },
    ],
  },
  {
    id: 'websearch',
    name: 'Web Search',
    category: 'web',
    descriptionEn: 'Search the live web for technical documentation, CVEs, APIs, and real-time information.',
    descriptionAr: 'البحث في شبكة الويب المباشرة عن التوثيقات التقنية والثغرات ومعلومات الأنظمة.',
    riskLevel: 'safe',
    iconName: 'SearchCode',
    commandExample: 'websearch(query: "CVE-2024-3094 xz-utils liblzma backdoor explanation")',
    parameters: [
      { name: 'query', type: 'string', required: true, desc: 'Search query keywords' },
      { name: 'domain', type: 'string', required: false, desc: 'Filter results to specific domain' },
    ],
  },
  {
    id: 'scrapling',
    name: 'Deep Web Scraper',
    category: 'web',
    descriptionEn: 'High-performance web scraper that bypasses bot mitigations and extracts structured DOM data.',
    descriptionAr: 'كاشط ويب متقدم يتجاوز حواجز الحماية ويستخرج عناصر DOM المنظمة بدقة عالية.',
    riskLevel: 'caution',
    iconName: 'FileCode',
    commandExample: 'scrapling(url: "https://example.com/api/v1/endpoints", css_selector: "table.routes tr")',
    parameters: [
      { name: 'url', type: 'string', required: true, desc: 'Target web URL' },
      { name: 'css_selector', type: 'string', required: false, desc: 'CSS selector query to filter DOM' },
    ],
  },
  {
    id: 'browser',
    name: 'Headless Browser',
    category: 'web',
    descriptionEn: 'Automate Chromium browser sessions: click, type, navigate, screenshot, and inspect SPAs.',
    descriptionAr: 'أتمتة جلسات متصفح كروميوم الكاملة: النقر، الكتابة، التنقل، والتقاط صور الشاشة.',
    riskLevel: 'caution',
    iconName: 'Monitor',
    commandExample: 'browser(action: "goto", url: "https://example.com/dashboard")',
    parameters: [
      { name: 'action', type: 'string', required: true, desc: "'goto' | 'click' | 'type' | 'screenshot' | 'evaluate'" },
      { name: 'url', type: 'string', required: false, desc: 'URL to navigate to' },
      { name: 'selector', type: 'string', required: false, desc: 'DOM element selector' },
    ],
  },
  {
    id: 'httpflow',
    name: 'HTTP Flow Interceptor',
    category: 'web',
    descriptionEn: 'Record, inspect, replay, and modify raw HTTP/HTTPS request and response traffic.',
    descriptionAr: 'تسجيل وفحص وتعديل وإعادة إرسال تدفق طلبات واستجابات HTTP/HTTPS.',
    riskLevel: 'caution',
    iconName: 'ArrowLeftRight',
    commandExample: 'httpflow(action: "replay", flow_id: "req_9921")',
    parameters: [
      { name: 'action', type: 'string', required: true, desc: "'capture' | 'replay' | 'diff' | 'list'" },
      { name: 'flow_id', type: 'string', required: false, desc: 'HTTP request flow ID' },
    ],
  },
  {
    id: 'jwt',
    name: 'JWT Security Tool',
    category: 'web',
    descriptionEn: 'Decode, inspect, verify, and test JWT tokens for weak secrets, "none" algorithm, and spoofing.',
    descriptionAr: 'فك وفحص واختبار رموز JWT الأمنية لكشف الخوارزميات الضعيفة وانتحال الهوية.',
    riskLevel: 'safe',
    iconName: 'Key',
    commandExample: 'jwt(token: "eyJhbGciOi...", action: "decode")',
    parameters: [
      { name: 'token', type: 'string', required: true, desc: 'Raw JWT token string' },
      { name: 'action', type: 'string', required: false, desc: "'decode' | 'verify' | 'tamper_none' | 'crack_secret'", defaultValue: 'decode' },
    ],
  },

  // ==========================================
  // E) SECURITY (20 Tools)
  // ==========================================
  {
    id: 'subfinder',
    name: 'Subfinder',
    category: 'security',
    descriptionEn: 'Passive subdomain discovery tool gathering valid subdomains from 40+ passive OSINT sources.',
    descriptionAr: 'أداة الاستكشاف السلبي للنطاقات الفرعية من أكثر من 40 مصدر OSINT خارجي.',
    riskLevel: 'safe',
    iconName: 'Radar',
    commandExample: 'subfinder(domain: "example.com", all_sources: true)',
    parameters: [
      { name: 'domain', type: 'string', required: true, desc: 'Root domain name (e.g. example.com)' },
      { name: 'all_sources', type: 'boolean', required: false, desc: 'Query all OSINT engines' },
    ],
  },
  {
    id: 'httpx',
    name: 'HTTPX Prober',
    category: 'security',
    descriptionEn: 'Fast and multi-purpose HTTP probe toolkit for probing URLs, status codes, tech stack, and CDN.',
    descriptionAr: 'أداة فحص واستطلاع HTTP متعددة الأغراض للتحقق من العناوين والتقنيات المستخدمة والـ CDN.',
    riskLevel: 'safe',
    iconName: 'Wifi',
    commandExample: 'httpx(targets: "subdomains.txt", ports: "80,443,8080,8443")',
    parameters: [
      { name: 'targets', type: 'string', required: true, desc: 'Target host or file path with list of hosts' },
      { name: 'ports', type: 'string', required: false, desc: 'Comma-separated port list to probe' },
    ],
  },
  {
    id: 'dnsx',
    name: 'DNSX Resolver',
    category: 'security',
    descriptionEn: 'Fast multi-purpose DNS resolution and probing tool with wildcard filtering and PTR lookup.',
    descriptionAr: 'محلل واستعلامات DNS فائق السرعة مع فلترة النطاقات العشوائية واستخراج سجلات PTR.',
    riskLevel: 'safe',
    iconName: 'Server',
    commandExample: 'dnsx(domain: "example.com", record_types: "A,AAAA,CNAME,MX")',
    parameters: [
      { name: 'domain', type: 'string', required: true, desc: 'Domain or host list' },
      { name: 'record_types', type: 'string', required: false, desc: 'DNS record types to query' },
    ],
  },
  {
    id: 'ffuf',
    name: 'FFUF Web Fuzzer',
    category: 'security',
    descriptionEn: 'Ultra-fast web fuzzer written in Go for discovering hidden directories, files, parameters, and vhosts.',
    descriptionAr: 'أداة فحص وتخمين مسارات الويب فائقة السرعة لاكتشاف المجلدات المخفية والمعاملات.',
    riskLevel: 'caution',
    iconName: 'Zap',
    commandExample: 'ffuf(url: "https://example.com/FUZZ", wordlist: "common.txt")',
    parameters: [
      { name: 'url', type: 'string', required: true, desc: 'Target URL containing the FUZZ placeholder' },
      { name: 'wordlist', type: 'string', required: true, desc: 'Wordlist path or name' },
      { name: 'filter_status', type: 'string', required: false, desc: 'Status codes to filter out (e.g. "404")' },
    ],
  },
  {
    id: 'katana',
    name: 'Katana Web Crawler',
    category: 'security',
    descriptionEn: 'Next-generation web crawler and spider for extracting links, forms, endpoints, and JS files.',
    descriptionAr: 'زاحف ويب متطور لاستخراج كافة الروابط والنماذج ونقاط الاتصال وملفات الجافاسكريبت.',
    riskLevel: 'caution',
    iconName: 'GitFork',
    commandExample: 'katana(url: "https://example.com", depth: 3, headless: true)',
    parameters: [
      { name: 'url', type: 'string', required: true, desc: 'Seed URL to begin crawling' },
      { name: 'depth', type: 'number', required: false, desc: 'Maximum crawl recursion depth' },
      { name: 'headless', type: 'boolean', required: false, desc: 'Enable headless browser execution' },
    ],
  },
  {
    id: 'waybackurls',
    name: 'WaybackURLs',
    category: 'security',
    descriptionEn: 'Fetch all historical URLs archived by Wayback Machine for target domains.',
    descriptionAr: 'جلب جميع الروابط التاريخية المؤرشفة في آلة الزمن (Wayback Machine) للنطاق المستهدف.',
    riskLevel: 'safe',
    iconName: 'Archive',
    commandExample: 'waybackurls(domain: "example.com")',
    parameters: [
      { name: 'domain', type: 'string', required: true, desc: 'Target domain' },
    ],
  },
  {
    id: 'gau',
    name: 'GetAllURLs (GAU)',
    category: 'security',
    descriptionEn: 'Fetch known URLs from AlienVault Open Threat Exchange, Wayback Machine, and Common Crawl.',
    descriptionAr: 'جلب كافة الروابط المعروفة من AlienVault OTX وآلة الزمن ومستودع Common Crawl.',
    riskLevel: 'safe',
    iconName: 'Compass',
    commandExample: 'gau(domain: "example.com", providers: "wayback,otx,commoncrawl")',
    parameters: [
      { name: 'domain', type: 'string', required: true, desc: 'Target domain' },
      { name: 'providers', type: 'string', required: false, desc: 'Comma-separated threat intelligence sources' },
    ],
  },
  {
    id: 'naabu',
    name: 'Naabu Port Scanner',
    category: 'security',
    descriptionEn: 'Fast SYN/CONNECT port scanner focused on reliability, low false positives, and integration.',
    descriptionAr: 'ماسح منافذ سريع بتقنية SYN/CONNECT مصمم للدقة العالية وقلة النتائج الإيجابية الخاطئة.',
    riskLevel: 'caution',
    iconName: 'Radio',
    commandExample: 'naabu(host: "10.0.4.15", ports: "top-100")',
    parameters: [
      { name: 'host', type: 'string', required: true, desc: 'Target IP address or hostname' },
      { name: 'ports', type: 'string', required: false, desc: 'Port ranges or "top-100", "top-1000"' },
    ],
  },
  {
    id: 'nmap',
    name: 'Nmap Network Scanner',
    category: 'security',
    descriptionEn: 'Industry-standard network exploration and security auditing tool with NSE scripts.',
    descriptionAr: 'أداة الفحص الشبكي القياسية عالميًا مع دعم كامل للاسكربتات التفاعلية NSE.',
    riskLevel: 'critical',
    iconName: 'Network',
    commandExample: 'nmap(target: "10.0.4.15", flags: "-sV -sC -T4")',
    parameters: [
      { name: 'target', type: 'string', required: true, desc: 'Target host or subnet' },
      { name: 'flags', type: 'string', required: false, desc: 'Nmap CLI switches (e.g. "-sV -p 80,443")' },
    ],
  },
  {
    id: 'nuclei',
    name: 'Nuclei Vulnerability Scanner',
    category: 'security',
    descriptionEn: 'Fast and template-based vulnerability scanner for scanning CVEs, misconfigurations, and exposures.',
    descriptionAr: 'أداة مسح الثغرات المعتمدة على القوالب لفحص الثغرات CVE والإعدادات غير الآمنة والتعرضات.',
    riskLevel: 'caution',
    iconName: 'ShieldAlert',
    commandExample: 'nuclei(target: "https://example.com", tags: "cve,misconfig")',
    parameters: [
      { name: 'target', type: 'string', required: true, desc: 'Target URL or IP' },
      { name: 'tags', type: 'string', required: false, desc: 'Filter templates by tags (e.g. "cve,rce,lfi")' },
      { name: 'severity', type: 'string', required: false, desc: 'critical, high, medium, low' },
    ],
  },
  {
    id: 'sqlmap',
    name: 'SQLMap Automatic SQLi',
    category: 'security',
    descriptionEn: 'Automatic SQL injection and database takeover tool detecting boolean, error, time, and union SQLi.',
    descriptionAr: 'أداة أوتوماتيكية متقدمة لاكتشاف واستغلال ثغرات حقن قواعد البيانات SQL Injection.',
    riskLevel: 'critical',
    iconName: 'Database',
    commandExample: 'sqlmap(url: "https://example.com/items?id=1", dbs: true)',
    parameters: [
      { name: 'url', type: 'string', required: true, desc: 'Target URL with parameters' },
      { name: 'data', type: 'string', required: false, desc: 'POST data body' },
      { name: 'dbs', type: 'boolean', required: false, desc: 'Enumerate database schemas' },
    ],
  },
  {
    id: 'dalfox',
    name: 'DalFox XSS Scanner',
    category: 'security',
    descriptionEn: 'Powerful parameter analysis and XSS scanner with DOM parsing and verification engine.',
    descriptionAr: 'أداة متطورة لتحليل المعاملات وفحص ثغرات XSS المتقاطعة مع التحقق الفعلي من الـ DOM.',
    riskLevel: 'caution',
    iconName: 'Bug',
    commandExample: 'dalfox(url: "https://example.com/search?q=test")',
    parameters: [
      { name: 'url', type: 'string', required: true, desc: 'Target URL with parameters' },
      { name: 'mining_dict', type: 'boolean', required: false, desc: 'Mine hidden parameters with dictionary' },
    ],
  },
  {
    id: 'corsy',
    name: 'Corsy CORS Scanner',
    category: 'security',
    descriptionEn: 'CORS misconfiguration scanner detecting Origin reflection, null origin trust, and pre-domain bypass.',
    descriptionAr: 'أداة فحص أخطاء إعدادات مشاركة الموارد عبر الأصول (CORS) والتحقق من سياسات الأمان.',
    riskLevel: 'safe',
    iconName: 'ShieldCheck',
    commandExample: 'corsy(url: "https://api.example.com")',
    parameters: [
      { name: 'url', type: 'string', required: true, desc: 'Target URL' },
      { name: 'headers', type: 'string', required: false, desc: 'Custom header JSON string' },
    ],
  },
  {
    id: 'crlfuzz',
    name: 'CRLFuzz',
    category: 'security',
    descriptionEn: 'Fast CRLF injection scanner discovering HTTP response splitting and header injection flaws.',
    descriptionAr: 'ماسح سريع لثغرات حقن CRLF وانشطار استجابة HTTP وحقن الترويسات غير المصرح بها.',
    riskLevel: 'caution',
    iconName: 'ShieldAlert',
    commandExample: 'crlfuzz(url: "https://example.com/redirect?url=test")',
    parameters: [
      { name: 'url', type: 'string', required: true, desc: 'Target URL' },
    ],
  },
  {
    id: 'cariddi',
    name: 'Cariddi Asset Scanner',
    category: 'security',
    descriptionEn: 'Scan URLs for leaked secrets, API keys, endpoints, and sensitive parameters.',
    descriptionAr: 'فحص الروابط بحثًا عن الأسرار والمفاتيح المسربة ونقاط الاتصال الحساسة والمعاملات.',
    riskLevel: 'safe',
    iconName: 'KeyRound',
    commandExample: 'cariddi(targets: "urls.txt")',
    parameters: [
      { name: 'targets', type: 'string', required: true, desc: 'Single URL or file path containing URLs' },
    ],
  },
  {
    id: 'assetfinder',
    name: 'Assetfinder',
    category: 'security',
    descriptionEn: 'Find domains and subdomains related to a given target from multiple OSINT registries.',
    descriptionAr: 'اكتشاف كافة النطاقات والأصول المرتبطة بالهدف من سجلات OSINT المتعددة.',
    riskLevel: 'safe',
    iconName: 'Target',
    commandExample: 'assetfinder(domain: "example.com", subs_only: true)',
    parameters: [
      { name: 'domain', type: 'string', required: true, desc: 'Root domain' },
      { name: 'subs_only', type: 'boolean', required: false, desc: 'Only output subdomains' },
    ],
  },
  {
    id: 'amass',
    name: 'OWASP Amass',
    category: 'security',
    descriptionEn: 'In-depth attack surface mapping and network infrastructure asset discovery engine.',
    descriptionAr: 'محرك OWASP لرسم خريطة سطح الهجوم واكتشاف الأصول والبنية التحتية للشبكة.',
    riskLevel: 'caution',
    iconName: 'Share2',
    commandExample: 'amass(domain: "example.com", active: false)',
    parameters: [
      { name: 'domain', type: 'string', required: true, desc: 'Target domain' },
      { name: 'active', type: 'boolean', required: false, desc: 'Enable active DNS queries' },
    ],
  },
  {
    id: 'authorization_review',
    name: 'AuthZ / BOLA Reviewer',
    category: 'security',
    descriptionEn: 'Automated authorization and broken object level authorization (BOLA/IDOR) matrix checker.',
    descriptionAr: 'محلل مصفوفة الصلاحيات وفحص ثغرات الإسناد المباشر غير الآمن للمكونات (BOLA/IDOR).',
    riskLevel: 'caution',
    iconName: 'Lock',
    commandExample: 'authorization_review(endpoint: "/api/v1/orders/{id}", user_tokens: ["tok_user1", "tok_user2"])',
    parameters: [
      { name: 'endpoint', type: 'string', required: true, desc: 'Endpoint URI template' },
      { name: 'user_tokens', type: 'array', required: true, desc: 'Array of auth tokens representing distinct roles' },
    ],
  },
  {
    id: 'ctf_tools',
    name: 'CTF Solver Toolkit',
    category: 'security',
    descriptionEn: 'Cryptographic decoders, binary unpackers, steganography tools, and pwntools helpers for CTFs.',
    descriptionAr: 'حزمة أدوات حل تحديات الأمن السيبراني: فك التشفير، تحليل الهندسة العكسية، والتعامل مع الثنائيات.',
    riskLevel: 'safe',
    iconName: 'Flag',
    commandExample: 'ctf_tools(module: "crypto", input_data: "416c706861436f6465")',
    parameters: [
      { name: 'module', type: 'string', required: true, desc: "'crypto' | 'pwn' | 'forensics' | 'rev' | 'web'" },
      { name: 'input_data', type: 'string', required: true, desc: 'Input string, ciphertext, or payload' },
    ],
  },
  {
    id: 'scope',
    name: 'Scope & Rules Enforcer',
    category: 'security',
    descriptionEn: 'Validate target hosts and IP ranges against authorization boundaries and rules of engagement.',
    descriptionAr: 'التحقق من مطابقة الأهداف وقواعد الاشتباك مع نطاق الفحص المصرح به وحمايته.',
    riskLevel: 'safe',
    iconName: 'Shield',
    commandExample: 'scope(target: "api.example.com", action: "check")',
    parameters: [
      { name: 'target', type: 'string', required: true, desc: 'Host, URL, or CIDR block to validate' },
      { name: 'action', type: 'string', required: false, desc: "'check' | 'add_in_scope' | 'add_out_of_scope'" },
    ],
  },
];

// Helper functions for easy consumption across the UI
export function getToolById(id: string): ToolDefinition | undefined {
  return TOOLS_DATABASE.find(t => t.id.toLowerCase() === id.toLowerCase());
}

export function getToolsByCategory(category: ToolCategory): ToolDefinition[] {
  return TOOLS_DATABASE.filter(t => t.category === category);
}

export function searchTools(
  query: string,
  category?: ToolCategory | 'all',
  risk?: ToolRiskLevel | 'all'
): ToolDefinition[] {
  const q = query.trim().toLowerCase();
  return TOOLS_DATABASE.filter(tool => {
    const matchesCategory = !category || category === 'all' || tool.category === category;
    const matchesRisk = !risk || risk === 'all' || tool.riskLevel === risk;
    if (!matchesCategory || !matchesRisk) return false;

    if (!q) return true;

    return (
      tool.id.toLowerCase().includes(q) ||
      tool.name.toLowerCase().includes(q) ||
      tool.descriptionEn.toLowerCase().includes(q) ||
      tool.descriptionAr.includes(q) ||
      tool.parameters.some(p => p.name.toLowerCase().includes(q) || p.desc.toLowerCase().includes(q))
    );
  });
}
