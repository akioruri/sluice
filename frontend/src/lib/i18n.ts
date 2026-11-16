import { derived, get, writable } from "svelte/store";

export type Locale = "en" | "zh" | "ja";

export const LOCALES: Locale[] = ["en", "zh", "ja"];

export const LOCALE_LABELS: Record<Locale, string> = {
  en: "English",
  zh: "中文",
  ja: "日本語",
};

const STORAGE_KEY = "sluice_locale";

/**
 * Picks the UI language: an explicit choice wins, then the browser's preferred
 * languages, then English.
 */
function detectLocale(): Locale {
  if (typeof localStorage !== "undefined") {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored && (LOCALES as string[]).includes(stored)) {
      return stored as Locale;
    }
  }

  if (typeof navigator !== "undefined") {
    const candidates: string[] =
      navigator.languages && navigator.languages.length > 0
        ? [...navigator.languages]
        : [navigator.language];

    for (const tag of candidates) {
      if (!tag) continue;
      const base = tag.toLowerCase().split("-")[0];
      if ((LOCALES as string[]).includes(base)) {
        return base as Locale;
      }
    }
  }

  return "en";
}

export const locale = writable<Locale>(detectLocale());

// Persist the choice and keep <html lang> in sync.
locale.subscribe((value) => {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, value);
  }
  if (typeof document !== "undefined") {
    document.documentElement.lang = value;
  }
});

export function setLocale(next: Locale): void {
  locale.set(next);
}

function interpolate(
  template: string,
  vars?: Record<string, string | number>,
): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, key) =>
    key in vars ? String(vars[key]) : match,
  );
}

export function translate(
  current: Locale,
  key: string,
  vars?: Record<string, string | number>,
): string {
  const table = messages[current] ?? messages.en;
  return interpolate(table[key] ?? messages.en[key] ?? key, vars);
}

/**
 * Reactive translator for templates: `{$t('nav.dashboard')}`.
 * Re-renders whenever the locale changes.
 */
export const t = derived(locale, ($locale) => {
  return (key: string, vars?: Record<string, string | number>) =>
    translate($locale, key, vars);
});

/** Non-reactive read for plain .ts modules (e.g. thrown error messages). */
export function tr(
  key: string,
  vars?: Record<string, string | number>,
): string {
  return translate(get(locale), key, vars);
}

const messages: Record<Locale, Record<string, string>> = {
  en: {
    "app.name": "Sluice Gateway",

    "common.loading": "Loading...",
    "common.save": "Save",
    "common.cancel": "Cancel",
    "common.copy": "Copy",
    "common.copied": "Copied",
    "common.active": "Active",
    "common.revoked": "Revoked",
    "common.edit": "Edit",
    "common.delete": "Delete",
    "common.name": "Name",
    "common.model": "Model",
    "common.selectModel": "Select a model",
    "common.noModels": "No models available",
    "common.required": "This field is required",

    "nav.dashboard": "Dashboard",
    "nav.vendors": "Vendors",
    "nav.tokens": "API Keys",
    "nav.stats": "Stats",
    "nav.signOut": "Sign Out",
    "nav.language": "Language",
    "nav.adminUser": "Administrator",

    "login.subtitle": "Sign in to manage the gateway",
    "login.username": "Username",
    "login.password": "Password",
    "login.submit": "Sign In",
    "login.submitting": "Signing in...",
    "login.required": "Username and password are required",
    "login.failed": "Login failed",
    "login.invalid": "Invalid username or password",

    "dashboard.title": "Dashboard",
    "dashboard.byVendor": "Requests by Vendor",
    "dashboard.noRequests": "No requests yet",
    "dashboard.subtitle": "Traffic and usage across every vendor and API key.",
    "dashboard.dailyByVendor": "Daily requests by vendor",
    "dashboard.dailyByKey": "Daily requests by API key",
    "dashboard.noChartData": "No usage data in the last 30 days",
    "dashboard.requests": "Requests",
    "dashboard.loadingChart": "Loading chart...",
    "dashboard.chartFailed": "Failed to load chart data",

    "stats.title": "Statistics",
    "stats.byVendor": "Usage by Vendor",
    "stats.noData": "No usage data yet",
    "stats.totalRequests": "Total Requests",
    "stats.activeVendors": "Active Vendors",
    "stats.activeKeys": "Active Keys",
    "stats.vendor": "Vendor",
    "stats.requests": "Requests",
    "stats.firstSeen": "First Seen",
    "stats.lastSeen": "Last Seen",
    "stats.loadFailed": "Failed to load stats",

    "vendors.title": "Vendors",
    "vendors.add": "Add Vendor",
    "vendors.edit": "Edit Vendor",
    "vendors.none": "No vendors configured",
    "vendors.subtitle":
      "Upstream model providers the gateway forwards requests to.",
    "vendors.apiKeyFirst":
      "Fill in the API key first, then fetch the model list.",
    "vendors.modelCount": "{count} models",
    "vendors.modelCountOne": "1 model",
    "vendors.name": "Name",
    "vendors.upstream": "Upstream URL",
    "vendors.authHeader": "Auth Header",
    "vendors.authFormat": "Auth Format",
    "vendors.modelsHint":
      "Comma-separated list of models this vendor serves. Leave it empty to allow every model.",
    "vendors.selectedCount": "{count} selected",
    "vendors.selectAll": "Select all",
    "vendors.clearAll": "Clear",
    "vendors.models": "Models",
    "vendors.searchModels": "Search models...",
    "vendors.manualModel": "Type a model name",
    "vendors.cat.text": "Text",
    "vendors.cat.reasoning": "Reasoning",
    "vendors.cat.vision": "Vision",
    "vendors.cat.image": "Image",
    "vendors.cat.audio": "Audio",
    "vendors.cat.embedding": "Embedding",
    "vendors.apiKey": "API Key",
    "vendors.apiKeyNew": "New API Key (optional)",
    "vendors.saving": "Saving...",
    "vendors.required":
      "Name, upstream, auth header, and auth format are required",
    "vendors.apiKeyRequired": "API key is required for new vendors",
    "vendors.loadFailed": "Failed to load vendors",
    "vendors.saveFailed": "Save failed",
    "vendors.deleteFailed": "Delete failed",
    "vendors.reserved":
      '"{name}" is a reserved name and cannot be used as a vendor',
    "vendors.actions": "Actions",
    "vendors.test": "Test",
    "vendors.testing": "Testing...",
    "vendors.testOk": "OK · {count} models · {ms}ms",
    "vendors.testFailed": "Failed",
    "vendors.testPartial": "{passed} of {total} models passed",
    "vendors.fetchModels": "Fetch Models",
    "vendors.fetching": "Fetching...",
    "vendors.fetchedCount": "Fetched {count} models",
    "vendors.noModels": "Upstream returned no models",
    "vendors.fetchFailed": "Failed to fetch models",

    "tokens.title": "API Keys",
    "tokens.generate": "Generate API Key",
    "tokens.generateHint":
      "The key is shown only once. Store it somewhere safe — only a prefix is kept on the server.",
    "tokens.generateBtn": "Generate Key",
    "tokens.generating": "Generating...",
    "tokens.newKey": "Your new API key",
    "tokens.howTo": "Call a model through the gateway",
    "tokens.howToHint":
      "Requests go to /{vendor}/... and the gateway swaps in the vendor's own key.",
    "tokens.key": "Key",
    "tokens.createdAt": "Created At",
    "tokens.status": "Status",
    "tokens.revoke": "Revoke",
    "tokens.revoking": "Revoking...",
    "tokens.none": "No keys yet",
    "tokens.nameRequired": "Please give this key a name",
    "tokens.nameDuplicate": "That name is already in use",
    "tokens.name": "Name",
    "tokens.loadFailed": "Failed to load keys",
    "tokens.createFailed": "Failed to create key",
    "tokens.revokeFailed": "Failed to revoke key",

    "error.unauthorized": "Session expired. Please sign in again.",
    "error.conflict": "A vendor with that name already exists.",
    "error.notFound": "Not found.",
    "error.invalidInput":
      "The server rejected these settings. Check the upstream URL starts with http:// or https://, and that the auth header name is valid.",
    "error.tooManyRequests": "Too many attempts. Please wait a minute.",
    "error.serviceUnavailable":
      "The gateway cannot serve this request: no database is configured, so tokens cannot be validated.",
    "error.requestFailed": "Request failed (HTTP {status})",
  },

  zh: {
    "app.name": "Sluice 网关",

    "common.loading": "加载中...",
    "common.save": "保存",
    "common.cancel": "取消",
    "common.copy": "复制",
    "common.copied": "已复制",
    "common.active": "有效",
    "common.revoked": "已吊销",
    "common.edit": "编辑",
    "common.delete": "删除",
    "common.name": "名称",
    "common.model": "模型",
    "common.selectModel": "请选择模型",
    "common.noModels": "暂无可用模型",
    "common.required": "此项为必填",

    "nav.dashboard": "仪表盘",
    "nav.vendors": "供应商",
    "nav.tokens": "API 密钥",
    "nav.stats": "统计",
    "nav.signOut": "退出登录",
    "nav.language": "语言",
    "nav.adminUser": "管理员",

    "login.subtitle": "登录以管理网关",
    "login.username": "用户名",
    "login.password": "密码",
    "login.submit": "登录",
    "login.submitting": "登录中...",
    "login.required": "用户名和密码为必填项",
    "login.failed": "登录失败",
    "login.invalid": "用户名或密码错误",

    "dashboard.title": "仪表盘",
    "dashboard.byVendor": "各供应商请求数",
    "dashboard.noRequests": "暂无请求",
    "dashboard.subtitle": "所有供应商与 API 密钥的流量和用量。",
    "dashboard.dailyByVendor": "每日各供应商请求数",
    "dashboard.dailyByKey": "每日各密钥请求数",
    "dashboard.noChartData": "近 30 天暂无用量数据",
    "dashboard.requests": "请求数",
    "dashboard.loadingChart": "图表加载中...",
    "dashboard.chartFailed": "图表数据加载失败",

    "stats.title": "统计",
    "stats.byVendor": "各供应商用量",
    "stats.noData": "暂无用量数据",
    "stats.totalRequests": "总请求数",
    "stats.activeVendors": "活跃供应商",
    "stats.activeKeys": "活跃密钥",
    "stats.vendor": "供应商",
    "stats.requests": "请求数",
    "stats.firstSeen": "首次出现",
    "stats.lastSeen": "最近出现",
    "stats.loadFailed": "加载统计失败",

    "vendors.title": "供应商",
    "vendors.add": "添加供应商",
    "vendors.edit": "编辑供应商",
    "vendors.none": "尚未配置供应商",
    "vendors.subtitle": "网关转发请求所依赖的上游模型供应商。",
    "vendors.apiKeyFirst": "请先填写 API 密钥，然后再获取模型列表。",
    "vendors.modelCount": "{count} 个模型",
    "vendors.modelCountOne": "1 个模型",
    "vendors.name": "名称",
    "vendors.upstream": "上游地址",
    "vendors.authHeader": "认证头",
    "vendors.authFormat": "认证格式",
    "vendors.modelsHint": "留空表示允许该供应商提供的所有模型",
    "vendors.selectedCount": "已选 {count} 个",
    "vendors.selectAll": "全选",
    "vendors.clearAll": "清空",
    "vendors.models": "模型",
    "vendors.searchModels": "搜索模型...",
    "vendors.manualModel": "手动输入模型名称",
    "vendors.cat.text": "文本",
    "vendors.cat.reasoning": "推理",
    "vendors.cat.vision": "视觉",
    "vendors.cat.image": "图像",
    "vendors.cat.audio": "语音",
    "vendors.cat.embedding": "向量",
    "vendors.apiKey": "API 密钥",
    "vendors.apiKeyNew": "新 API 密钥（可选）",
    "vendors.saving": "保存中...",
    "vendors.required": "名称、上游地址、认证头和认证格式为必填项",
    "vendors.apiKeyRequired": "新供应商必须填写 API 密钥",
    "vendors.loadFailed": "加载供应商失败",
    "vendors.saveFailed": "保存失败",
    "vendors.deleteFailed": "删除失败",
    "vendors.reserved": "“{name}”是保留名称，不能用作供应商名称",
    "vendors.actions": "操作",
    "vendors.test": "测试",
    "vendors.testing": "测试中...",
    "vendors.testOk": "成功 · {count} 个模型 · {ms}ms",
    "vendors.testFailed": "失败",
    "vendors.testPartial": "{total} 个模型中 {passed} 个通过",
    "vendors.fetchModels": "获取模型",
    "vendors.fetching": "获取中...",
    "vendors.fetchedCount": "已获取 {count} 个模型",
    "vendors.noModels": "上游未返回任何模型",
    "vendors.fetchFailed": "获取模型失败",

    "tokens.title": "API 密钥",
    "tokens.generate": "生成 API 密钥",
    "tokens.generateHint": "密钥仅显示一次，请妥善保存。服务端只保留前缀。",
    "tokens.generateBtn": "生成密钥",
    "tokens.generating": "生成中...",
    "tokens.newKey": "你的新 API 密钥",
    "tokens.howTo": "通过网关调用模型",
    "tokens.howToHint":
      "请求发往 /{vendor}/...，网关会自动替换为该供应商自己的密钥。",
    "tokens.key": "密钥",
    "tokens.createdAt": "创建时间",
    "tokens.status": "状态",
    "tokens.revoke": "吊销",
    "tokens.revoking": "吊销中...",
    "tokens.none": "暂无密钥",
    "tokens.nameRequired": "请为该密钥填写名称",
    "tokens.nameDuplicate": "该名称已被使用",
    "tokens.name": "名称",
    "tokens.loadFailed": "加载密钥失败",
    "tokens.createFailed": "创建密钥失败",
    "tokens.revokeFailed": "吊销密钥失败",

    "error.unauthorized": "会话已过期，请重新登录。",
    "error.conflict": "已存在同名的供应商。",
    "error.notFound": "未找到。",
    "error.invalidInput":
      "服务器拒绝了这些设置。请检查上游地址以 http:// 或 https:// 开头，且认证头名称有效。",
    "error.tooManyRequests": "尝试次数过多，请稍等一分钟。",
    "error.serviceUnavailable":
      "网关无法处理该请求：未配置数据库，无法校验令牌。",
    "error.requestFailed": "请求失败（HTTP {status}）",
  },

  ja: {
    "app.name": "Sluice ゲートウェイ",

    "common.loading": "読み込み中...",
    "common.save": "保存",
    "common.cancel": "キャンセル",
    "common.copy": "コピー",
    "common.copied": "コピーしました",
    "common.active": "有効",
    "common.revoked": "失効済み",
    "common.edit": "編集",
    "common.delete": "削除",
    "common.name": "名前",
    "common.model": "モデル",
    "common.selectModel": "モデルを選択",
    "common.noModels": "利用可能なモデルがありません",
    "common.required": "この項目は必須です",

    "nav.dashboard": "ダッシュボード",
    "nav.vendors": "ベンダー",
    "nav.tokens": "API キー",
    "nav.stats": "統計",
    "nav.signOut": "ログアウト",
    "nav.language": "言語",
    "nav.adminUser": "管理者",

    "login.subtitle": "ゲートウェイ管理にログイン",
    "login.username": "ユーザー名",
    "login.password": "パスワード",
    "login.submit": "ログイン",
    "login.submitting": "ログイン中...",
    "login.required": "ユーザー名とパスワードを入力してください",
    "login.failed": "ログインに失敗しました",
    "login.invalid": "ユーザー名またはパスワードが正しくありません",

    "dashboard.title": "ダッシュボード",
    "dashboard.byVendor": "ベンダー別リクエスト数",
    "dashboard.noRequests": "リクエストはまだありません",
    "dashboard.subtitle": "全ベンダーと API キーのトラフィックと使用状況。",
    "dashboard.dailyByVendor": "ベンダー別日次リクエスト数",
    "dashboard.dailyByKey": "API キー別日次リクエスト数",
    "dashboard.noChartData": "過去 30 日間に使用データがありません",
    "dashboard.requests": "リクエスト数",
    "dashboard.loadingChart": "グラフを読み込み中...",
    "dashboard.chartFailed": "グラフデータの読み込みに失敗しました",

    "stats.title": "統計",
    "stats.byVendor": "ベンダー別使用状況",
    "stats.noData": "使用データはまだありません",
    "stats.totalRequests": "総リクエスト数",
    "stats.activeVendors": "有効なベンダー",
    "stats.activeKeys": "有効なキー",
    "stats.vendor": "ベンダー",
    "stats.requests": "リクエスト数",
    "stats.firstSeen": "初回",
    "stats.lastSeen": "最終",
    "stats.loadFailed": "統計の読み込みに失敗しました",

    "vendors.title": "ベンダー",
    "vendors.add": "ベンダーを追加",
    "vendors.edit": "ベンダーを編集",
    "vendors.none": "ベンダーが未設定です",
    "vendors.subtitle":
      "ゲートウェイがリクエストを転送する上流モデルベンダー。",
    "vendors.apiKeyFirst":
      "先に API キーを入力してからモデル一覧を取得してください。",
    "vendors.modelCount": "{count} 個のモデル",
    "vendors.modelCountOne": "1 個のモデル",
    "vendors.name": "名前",
    "vendors.upstream": "上流 URL",
    "vendors.authHeader": "認証ヘッダー",
    "vendors.authFormat": "認証フォーマット",
    "vendors.modelsHint":
      "このベンダーが提供するモデルのカンマ区切りリスト。空にするとすべてのモデルを許可します。",
    "vendors.selectedCount": "{count} 件選択中",
    "vendors.selectAll": "すべて選択",
    "vendors.clearAll": "クリア",
    "vendors.models": "モデル",
    "vendors.searchModels": "モデルを検索...",
    "vendors.manualModel": "モデル名を手動入力",
    "vendors.cat.text": "テキスト",
    "vendors.cat.reasoning": "推論",
    "vendors.cat.vision": "ビジョン",
    "vendors.cat.image": "画像",
    "vendors.cat.audio": "音声",
    "vendors.cat.embedding": "埋め込み",
    "vendors.apiKey": "API キー",
    "vendors.apiKeyNew": "新しい API キー（任意）",
    "vendors.saving": "保存中...",
    "vendors.required":
      "名前、上流 URL、認証ヘッダー、認証フォーマットは必須です",
    "vendors.apiKeyRequired": "新しいベンダーには API キーが必要です",
    "vendors.loadFailed": "ベンダーの読み込みに失敗しました",
    "vendors.saveFailed": "保存に失敗しました",
    "vendors.deleteFailed": "削除に失敗しました",
    "vendors.reserved":
      "「{name}」は予約名のため、ベンダー名として使用できません",
    "vendors.actions": "操作",
    "vendors.test": "テスト",
    "vendors.testing": "テスト中...",
    "vendors.testOk": "成功 · {count} 個のモデル · {ms}ms",
    "vendors.testFailed": "失敗",
    "vendors.testPartial": "{total} 個中 {passed} 個が成功",
    "vendors.fetchModels": "モデルを取得",
    "vendors.fetching": "取得中...",
    "vendors.fetchedCount": "{count} 個のモデルを取得しました",
    "vendors.noModels": "上流からモデルが返されませんでした",
    "vendors.fetchFailed": "モデルの取得に失敗しました",

    "tokens.title": "API キー",
    "tokens.generate": "API キーを生成",
    "tokens.generateHint":
      "キーは一度だけ表示されます。安全に保管してください。サーバーには接頭辞のみ保存されます。",
    "tokens.generateBtn": "キーを生成",
    "tokens.generating": "生成中...",
    "tokens.newKey": "新しい API キー",
    "tokens.howTo": "ゲートウェイ経由でモデルを呼び出す",
    "tokens.howToHint":
      "リクエストは /{vendor}/... に送信され、ゲートウェイがベンダー固有のキーに置き換えます。",
    "tokens.key": "キー",
    "tokens.createdAt": "作成日時",
    "tokens.status": "状態",
    "tokens.revoke": "失効",
    "tokens.revoking": "失効処理中...",
    "tokens.none": "キーはまだありません",
    "tokens.nameRequired": "キーに名前を付けてください",
    "tokens.nameDuplicate": "その名前は既に使用されています",
    "tokens.name": "名前",
    "tokens.loadFailed": "キーの読み込みに失敗しました",
    "tokens.createFailed": "キーの作成に失敗しました",
    "tokens.revokeFailed": "キーの失効に失敗しました",

    "error.unauthorized":
      "セッションの有効期限が切れました。再度ログインしてください。",
    "error.conflict": "同じ名前のベンダーが既に存在します。",
    "error.notFound": "見つかりません。",
    "error.invalidInput":
      "サーバーがこれらの設定を拒否しました。上流 URL が http:// または https:// で始まり、認証ヘッダー名が有効であることを確認してください。",
    "error.tooManyRequests": "試行回数が多すぎます。1 分ほどお待ちください。",
    "error.serviceUnavailable":
      "ゲートウェイはこのリクエストを処理できません：データベースが未設定のため、トークンを検証できません。",
    "error.requestFailed": "リクエスト失敗（HTTP {status}）",
  },
};
