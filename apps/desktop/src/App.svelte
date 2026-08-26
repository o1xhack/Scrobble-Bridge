<script lang="ts">
  import {
    classifySyncReport,
    dateLocale,
    LOCALE_STORAGE_KEY,
    resolveLocale,
    type ActivityEntry,
    type ActivityPage,
    type ActivityStatus as ActivityStatusValue,
    type Locale,
    type RuntimePhase,
    type RuntimeStatus,
    type SyncReport,
  } from "@scrobble-bridge/ui";
  import { listen } from "@tauri-apps/api/event";
  import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { SiLastdotfm, SiYoutubemusic } from "@icons-pack/svelte-simple-icons";
  import {
    ArrowLeft,
    ArrowRight,
    ArrowsClockwise,
    Browser,
    CaretDown,
    CaretLeft,
    CaretRight,
    CheckCircle,
    Clock,
    DownloadSimple,
    Export,
    Gear,
    Key,
    MagnifyingGlass,
    Pause,
    Play,
    WarningCircle,
    X,
  } from "phosphor-svelte";
  import { onMount } from "svelte";
  import ActivityList from "./ActivityList.svelte";
  import ActivityStatus from "./ActivityStatus.svelte";
  import { desktop, isDemoMode, type SoftwareUpdateStatus } from "./transport";

  type View = "dashboard" | "history" | "settings";

  const COPY = {
    en: {
      runtime: "YouTube Music → Last.fm",
      back: "Back",
      settings: "Settings",
      syncNow: "Sync now",
      syncing: "Syncing…",
      syncComplete: "Sync complete.",
      syncBaseline: "History baseline saved. New plays will sync from now on.",
      syncNoChanges:
        "Checked successfully. No new listening activity was found.",
      syncAccepted: "Synced {count} track(s) to Last.fm.",
      syncGap:
        "YouTube Music history could not be safely aligned. Nothing new was sent to Last.fm; automatic retries will continue.",
      syncIncomplete:
        "Some tracks were not accepted by Last.fm. They remain queued for retry or need attention.",
      nextCheck: "Next check",
      source: "YouTube Music",
      destination: "Last.fm",
      connected: "Connected",
      notConnected: "Not connected",
      accountUnavailable: "Account name unavailable",
      openSettings: "Open settings",
      bridgeReady: "Your bridge is ready",
      bridgeNeedsSetup: "Finish connecting your bridge",
      bridgeReadyBody:
        "Listening history is checked quietly in the background and sent to Last.fm.",
      bridgeNeedsSetupBody:
        "Connect both services in Settings to start syncing your listening history.",
      recentActivity: "Recent activity",
      recentActivityBody:
        "One song per row. Select a song to inspect its full sync lifecycle.",
      allHistory: "View all history",
      emptyTitle: "No listening activity yet",
      emptyBody:
        "Play something in YouTube Music, then run a sync. Your newest songs will appear here.",
      background: "Scrobble Bridge keeps running when this window is closed.",
      history: "Sync history",
      historyBody:
        "Search and review your complete local history without loading it all at once.",
      search: "Search title or artist",
      allStatuses: "All statuses",
      previous: "Previous",
      next: "Next",
      showing: "Showing",
      of: "of",
      noResults: "No matching songs",
      settingsTitle: "Settings",
      settingsBody:
        "Connections, background behavior, language, and recovery tools.",
      connections: "Connections",
      refreshAccount: "Refresh account name",
      refreshing: "Refreshing…",
      extensionManaged:
        "Sign-in is maintained by the Chrome extension while Chrome is open.",
      configureApplication: "Add Last.fm API application",
      changeApplication: "Change API application",
      lastFmApplicationReady:
        "Scrobble Bridge is ready. Sign in to Last.fm to connect your account.",
      lastFmAuthorize: "Authorize with Last.fm",
      lastFmFinish: "I approved access — finish connection",
      lastFmReauthorize: "Reconnect Last.fm",
      apiKey: "API key",
      sharedSecret: "Shared secret",
      saveAndAuthorize: "Save and authorize",
      preferences: "Preferences",
      language: "Language",
      languageBody: "Choose the language used by the desktop App.",
      launchAtLogin: "Launch at login",
      launchAtLoginBody:
        "Start the sync service automatically after signing in.",
      syncService: "Sync service",
      paused: "Paused",
      active: "Running",
      pause: "Pause",
      resume: "Resume",
      diagnostics: "Diagnostics",
      diagnosticsBody:
        "Export a private support package without Cookie or API secrets.",
      export: "Export diagnostics",
      savedTo: "Saved to",
      softwareUpdates: "Software updates",
      dailyUpdateBody:
        "Checks automatically once a day. Updates are never downloaded or installed without your choice.",
      currentVersion: "Current version",
      lastUpdateCheck: "Last checked",
      nextUpdateCheck: "Next automatic check",
      checkForUpdates: "Check now",
      checkingUpdates: "Checking…",
      updateAvailable: "A new version is available",
      updateAvailableBody: "Scrobble Bridge {version} is ready to download.",
      downloadUpdate: "Download update",
      downloadingUpdate: "Downloading…",
      updateReady: "Downloaded and verified",
      updateNowRestart: "Update now and restart",
      installingUpdate: "Installing…",
      later: "Later",
      releaseNotes: "What’s new",
      upToDate: "Scrobble Bridge is up to date.",
      updateCheckFailed: "Could not check for updates.",
      updateDownloadFailed: "Could not download the update.",
      updateInstallFailed: "Could not install the update.",
      advanced: "Advanced recovery",
      advancedBody:
        "Manual credentials are only for recovery when the Chrome extension cannot connect.",
      accountId: "Account label",
      authUser: "Google account index",
      cookie: "YouTube Music Cookie header",
      saveYouTube: "Save manual credentials",
      manualSaved: "Manual YouTube Music credentials saved.",
      authorizationOpened: "Last.fm authorization opened in Chrome.",
      lastFmConnected: "Last.fm connected.",
      identityUpdated: "YouTube Music account updated.",
      pausedMessage: "Sync paused.",
      resumedMessage: "Sync resumed.",
      activityDetail: "Sync details",
      listened: "Listened",
      discovered: "Discovered locally",
      lastUpdated: "Last updated",
      attempts: "Submission attempts",
      sourceTrack: "Source track",
      destinationState: "Last.fm result",
      error: "Last error",
      noError: "None",
      close: "Close",
      notYet: "Not yet",
      justNow: "now",
      page: "Page",
      failedToLoad: "Could not load the sync service.",
      youtubeAuthorizationExpired:
        "YouTube Music sign-in expired. Open Chrome, sign in again, and let the extension refresh access.",
      lastFmAuthorizationExpired:
        "Last.fm authorization expired. Reconnect your account in Settings.",
      youtubeHistoryChanged:
        "YouTube Music changed its history format. Syncing stopped safely until the app is updated.",
      temporaryUnavailable:
        "The music service is temporarily unavailable. Another sync is scheduled.",
    },
    "zh-CN": {
      runtime: "YouTube Music → Last.fm 同步",
      back: "返回",
      settings: "设置",
      syncNow: "立即同步",
      syncing: "正在同步…",
      syncComplete: "同步完成。",
      syncBaseline: "已保存历史基准；之后出现的新播放会自动同步。",
      syncNoChanges: "检查正常，没有发现新的收听记录。",
      syncAccepted: "已成功同步 {count} 首歌曲到 Last.fm。",
      syncGap:
        "YouTube Music 历史无法安全衔接，本轮没有向 Last.fm 提交新记录；程序会继续自动重试。",
      syncIncomplete: "部分歌曲尚未被 Last.fm 接受，已保留等待重试或需要处理。",
      nextCheck: "下次检查",
      source: "YouTube Music",
      destination: "Last.fm",
      connected: "已连接",
      notConnected: "未连接",
      accountUnavailable: "暂未取得账号名称",
      openSettings: "打开设置",
      bridgeReady: "同步桥已准备好",
      bridgeNeedsSetup: "还需完成连接",
      bridgeReadyBody: "程序会在后台安静地检查收听记录，并同步到 Last.fm。",
      bridgeNeedsSetupBody:
        "请在设置中连接两个服务，之后即可开始同步收听记录。",
      recentActivity: "最近同步",
      recentActivityBody: "一首歌一行；点击歌曲可查看从发现到提交的完整过程。",
      allHistory: "查看全部历史",
      emptyTitle: "还没有收听记录",
      emptyBody:
        "在 YouTube Music 播放一首歌，然后立即同步；最新歌曲会显示在这里。",
      background: "关闭窗口后，Scrobble Bridge 仍会在后台运行。",
      history: "同步历史",
      historyBody: "分页搜索完整的本地历史，不会一次加载几百或几千首歌。",
      search: "搜索歌曲或歌手",
      allStatuses: "全部状态",
      previous: "上一页",
      next: "下一页",
      showing: "正在显示",
      of: "共",
      noResults: "没有符合条件的歌曲",
      settingsTitle: "设置",
      settingsBody: "统一管理连接、后台运行、语言和恢复工具。",
      connections: "服务连接",
      refreshAccount: "刷新账号名称",
      refreshing: "正在刷新…",
      extensionManaged:
        "Chrome 打开时，扩展会维护登录状态；关闭 Chrome 后同步仍可继续。",
      configureApplication: "添加 Last.fm API 应用",
      changeApplication: "更换 API 应用",
      lastFmApplicationReady:
        "Scrobble Bridge 已准备就绪，登录 Last.fm 后即可连接。",
      lastFmAuthorize: "前往 Last.fm 授权",
      lastFmFinish: "我已批准访问，完成连接",
      lastFmReauthorize: "重新连接 Last.fm",
      apiKey: "API Key",
      sharedSecret: "Shared Secret",
      saveAndAuthorize: "保存并授权",
      preferences: "偏好设置",
      language: "语言",
      languageBody: "选择桌面 App 显示的语言。",
      launchAtLogin: "登录时启动",
      launchAtLoginBody: "登录电脑后自动启动同步服务。",
      syncService: "同步服务",
      paused: "已暂停",
      active: "正在运行",
      pause: "暂停",
      resume: "继续",
      diagnostics: "诊断信息",
      diagnosticsBody: "导出不包含 Cookie 或 API 密钥的隐私安全支持包。",
      export: "导出诊断信息",
      savedTo: "已保存到",
      softwareUpdates: "软件更新",
      dailyUpdateBody: "每天自动检查一次；未经你选择，不会自动下载或安装更新。",
      currentVersion: "当前版本",
      lastUpdateCheck: "上次检查",
      nextUpdateCheck: "下次自动检查",
      checkForUpdates: "立即检查",
      checkingUpdates: "正在检查…",
      updateAvailable: "发现新版本",
      updateAvailableBody: "Scrobble Bridge {version} 已可下载。",
      downloadUpdate: "下载更新",
      downloadingUpdate: "正在下载…",
      updateReady: "已下载并验证",
      updateNowRestart: "立即更新并重启",
      installingUpdate: "正在安装…",
      later: "稍后提醒",
      releaseNotes: "更新说明",
      upToDate: "Scrobble Bridge 已是最新版本。",
      updateCheckFailed: "无法检查软件更新。",
      updateDownloadFailed: "无法下载更新。",
      updateInstallFailed: "无法安装更新。",
      advanced: "高级恢复",
      advancedBody: "只有 Chrome 扩展无法连接时，才需要手动填写凭据。",
      accountId: "账号标签",
      authUser: "Google 账号序号",
      cookie: "YouTube Music Cookie 请求头",
      saveYouTube: "保存手动凭据",
      manualSaved: "已保存手动 YouTube Music 凭据。",
      authorizationOpened: "已在 Chrome 打开 Last.fm 授权页面。",
      lastFmConnected: "Last.fm 已连接。",
      identityUpdated: "YouTube Music 账号信息已更新。",
      pausedMessage: "同步已暂停。",
      resumedMessage: "同步已继续。",
      activityDetail: "同步详情",
      listened: "收听时间",
      discovered: "本地发现时间",
      lastUpdated: "最后更新时间",
      attempts: "提交次数",
      sourceTrack: "来源歌曲",
      destinationState: "Last.fm 结果",
      error: "最近错误",
      noError: "无",
      close: "关闭",
      notYet: "尚未发生",
      justNow: "刚刚",
      page: "第",
      failedToLoad: "无法载入同步服务。",
      youtubeAuthorizationExpired:
        "YouTube Music 登录状态已过期。请打开 Chrome，重新登录并等待扩展刷新授权。",
      lastFmAuthorizationExpired:
        "Last.fm 授权已失效，请在设置中重新连接账号。",
      youtubeHistoryChanged:
        "YouTube Music 调整了历史记录格式；为避免错误同步，程序已暂停并需要更新。",
      temporaryUnavailable:
        "音乐服务暂时无法访问，程序会在下次检查时自动重试。",
    },
  } as const;

  const EMPTY_PAGE: ActivityPage = {
    items: [],
    total: 0,
    limit: 50,
    offset: 0,
  };
  const STATUS_OPTIONS: (ActivityStatusValue | "")[] = [
    "",
    "accepted",
    "pending",
    "submitting",
    "retryable",
    "rejected",
  ];

  let locale = $state<Locale>("en");
  let text = $derived(COPY[locale]);
  let view = $state<View>("dashboard");
  let status = $state<RuntimeStatus | null>(null);
  let recent = $state<ActivityPage>(EMPTY_PAGE);
  let softwareUpdate = $state<SoftwareUpdateStatus | null>(null);
  let history = $state<ActivityPage>(EMPTY_PAGE);
  let historySearch = $state("");
  let historyStatus = $state<ActivityStatusValue | "">("");
  let historyOffset = $state(0);
  let selectedActivity = $state<ActivityEntry | null>(null);
  let launchAtLogin = $state(false);
  let busy = $state(false);
  let identityBusy = $state(false);
  let updateBusy = $state(false);
  let dismissedUpdateVersion = $state<string | null>(null);
  let identityRefreshAttempted = false;
  let message = $state("");
  let errorMessage = $state("");
  let showLastFmForm = $state(false);
  let authorizationStarted = $state(false);
  let showAdvanced = $state(false);
  let apiKey = $state("");
  let sharedSecret = $state("");
  let accountId = $state("default");
  let authUser = $state(0);
  let cookieHeader = $state("");

  onMount(() => {
    selectLocale(
      resolveLocale(
        localStorage.getItem(LOCALE_STORAGE_KEY),
        navigator.language,
      ),
      false,
    );
    void refreshAll();
    void refreshSoftwareUpdateStatus();
    if (isDemoMode) launchAtLogin = true;
    else void isEnabled().then((value) => (launchAtLogin = value));
    const timer = window.setInterval(() => {
      void refreshAll(true);
      void refreshSoftwareUpdateStatus(true);
    }, 10_000);
    const finishAuthorizationOnReturn = () => {
      if (authorizationStarted && !busy) void finishLastFm(true);
    };
    window.addEventListener("focus", finishAuthorizationOnReturn);
    const unlisten = isDemoMode
      ? null
      : listen("open-diagnostics", () => {
          view = "settings";
          showAdvanced = true;
        });
    const unlistenUpdate = isDemoMode
      ? null
      : listen<SoftwareUpdateStatus>("software-update-changed", (event) => {
          softwareUpdate = event.payload;
        });
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", finishAuthorizationOnReturn);
      if (unlisten) void unlisten.then((dispose) => dispose());
      if (unlistenUpdate) void unlistenUpdate.then((dispose) => dispose());
    };
  });

  function selectLocale(next: Locale, persist = true) {
    locale = next;
    document.documentElement.lang = next;
    if (persist) localStorage.setItem(LOCALE_STORAGE_KEY, next);
  }

  async function refreshAll(silent = false) {
    try {
      const [nextStatus, nextRecent] = await Promise.all([
        desktop.status(),
        desktop.activity(50, 0),
      ]);
      status = nextStatus;
      recent = nextRecent;
      if (view === "history") await loadHistory(true);
      if (
        nextStatus.ytmusic_configured &&
        !nextStatus.ytmusic_account_name &&
        !identityRefreshAttempted
      ) {
        identityRefreshAttempted = true;
        void refreshIdentity(true);
      }
      if (!silent) errorMessage = "";
    } catch (error) {
      if (!silent) errorMessage = `${text.failedToLoad} ${String(error)}`;
    }
  }

  async function refreshSoftwareUpdateStatus(silent = false) {
    try {
      softwareUpdate = await desktop.updateStatus();
    } catch (error) {
      if (!silent) errorMessage = `${text.updateCheckFailed} ${String(error)}`;
    }
  }

  async function checkForUpdate() {
    updateBusy = true;
    message = "";
    errorMessage = "";
    try {
      softwareUpdate = await desktop.checkForUpdate();
      if (softwareUpdate.available_version) {
        dismissedUpdateVersion = null;
      } else {
        message = text.upToDate;
      }
    } catch (error) {
      await refreshSoftwareUpdateStatus(true);
      errorMessage = `${text.updateCheckFailed} ${String(error)}`;
    } finally {
      updateBusy = false;
    }
  }

  async function downloadUpdate() {
    updateBusy = true;
    message = "";
    errorMessage = "";
    try {
      softwareUpdate = await desktop.downloadUpdate();
    } catch (error) {
      await refreshSoftwareUpdateStatus(true);
      errorMessage = `${text.updateDownloadFailed} ${String(error)}`;
    } finally {
      updateBusy = false;
    }
  }

  async function installUpdate() {
    updateBusy = true;
    message = "";
    errorMessage = "";
    try {
      await desktop.installUpdate();
    } catch (error) {
      await refreshSoftwareUpdateStatus(true);
      errorMessage = `${text.updateInstallFailed} ${String(error)}`;
      updateBusy = false;
    }
  }

  async function run(action: () => Promise<unknown>, success: string) {
    busy = true;
    message = "";
    errorMessage = "";
    try {
      await action();
      message = success;
      await refreshAll(true);
    } catch (error) {
      errorMessage = String(error);
    } finally {
      busy = false;
    }
  }

  function syncFeedbackText(report: SyncReport) {
    const feedback = classifySyncReport(report);
    switch (feedback.reason) {
      case "history_gap":
        return { error: true, value: text.syncGap };
      case "submission_incomplete":
        return { error: true, value: text.syncIncomplete };
      case "baseline":
        return { error: false, value: text.syncBaseline };
      case "synced":
        return {
          error: false,
          value: text.syncAccepted.replace("{count}", String(feedback.count)),
        };
      case "no_changes":
        return { error: false, value: text.syncNoChanges };
    }
  }

  async function runSync() {
    busy = true;
    message = "";
    errorMessage = "";
    try {
      const feedback = syncFeedbackText(await desktop.sync());
      if (feedback.error) errorMessage = feedback.value;
      else message = feedback.value;
      await refreshAll(true);
    } catch (error) {
      errorMessage = String(error);
    } finally {
      busy = false;
    }
  }

  function runtimeDetail(current: RuntimeStatus) {
    if (current.last_error_code === "history_gap") return text.syncGap;
    if (current.last_error_code === "submission_incomplete")
      return text.syncIncomplete;
    if (current.last_error_code === "ytmusic_auth")
      return text.youtubeAuthorizationExpired;
    if (current.last_error_code === "lastfm_auth")
      return text.lastFmAuthorizationExpired;
    if (current.last_error_code === "ytmusic_schema")
      return text.youtubeHistoryChanged;
    if (
      current.last_error_code === "ytmusic_unavailable" ||
      current.last_error_code === "lastfm_temporary"
    )
      return text.temporaryUnavailable;
    return `${text.nextCheck}: ${formatTime(current.next_scheduled_at)}`;
  }

  async function openView(next: View) {
    view = next;
    selectedActivity = null;
    if (next === "history") await loadHistory();
  }

  async function loadHistory(silent = false) {
    try {
      history = await desktop.activity(
        50,
        historyOffset,
        historySearch.trim() || undefined,
        historyStatus || undefined,
      );
      if (!silent) errorMessage = "";
    } catch (error) {
      if (!silent) errorMessage = String(error);
    }
  }

  async function applyHistoryFilters() {
    historyOffset = 0;
    await loadHistory();
  }

  async function changePage(direction: -1 | 1) {
    historyOffset = Math.max(0, historyOffset + direction * 50);
    await loadHistory();
  }

  async function refreshIdentity(silent = false) {
    identityBusy = true;
    if (!silent) {
      message = "";
      errorMessage = "";
    }
    try {
      await desktop.refreshYouTubeMusicIdentity();
      await refreshAll(true);
      if (!silent) message = text.identityUpdated;
    } catch (error) {
      if (!silent) errorMessage = String(error);
    } finally {
      identityBusy = false;
    }
  }

  async function toggleAutostart() {
    const next = !launchAtLogin;
    if (isDemoMode) {
      launchAtLogin = next;
      return;
    }
    try {
      if (next) await enable();
      else await disable();
      launchAtLogin = next;
    } catch (error) {
      errorMessage = String(error);
    }
  }

  async function openLastFmAuthorization(saveApplication = false) {
    busy = true;
    message = "";
    errorMessage = "";
    try {
      if (saveApplication) {
        await desktop.saveLastFmApplication(apiKey.trim(), sharedSecret.trim());
        apiKey = "";
        sharedSecret = "";
        showLastFmForm = false;
      }
      const url = await desktop.startLastFm();
      if (!isDemoMode) await openUrl(url);
      authorizationStarted = true;
      message = text.authorizationOpened;
      await refreshAll(true);
    } catch (error) {
      errorMessage = String(error);
    } finally {
      busy = false;
    }
  }

  async function finishLastFm(silent = false) {
    if (!silent) {
      await run(async () => {
        await desktop.finishLastFm();
        authorizationStarted = false;
      }, text.lastFmConnected);
      return;
    }

    busy = true;
    try {
      await desktop.finishLastFm();
      authorizationStarted = false;
      message = text.lastFmConnected;
      errorMessage = "";
      await refreshAll(true);
    } catch {
      // Returning before the browser approval finishes is harmless; the
      // explicit button remains available and reports genuine failures.
    } finally {
      busy = false;
    }
  }

  async function exportDiagnostics() {
    busy = true;
    message = "";
    errorMessage = "";
    try {
      message = `${text.savedTo} ${await desktop.exportDiagnostics()}`;
    } catch (error) {
      errorMessage = String(error);
    } finally {
      busy = false;
    }
  }

  function formatDate(value: string | null | undefined) {
    return value
      ? new Intl.DateTimeFormat(dateLocale(locale), {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(new Date(value))
      : text.notYet;
  }

  function formatTime(value: string | null | undefined) {
    return value
      ? new Intl.DateTimeFormat(dateLocale(locale), {
          hour: "2-digit",
          minute: "2-digit",
        }).format(new Date(value))
      : "—";
  }

  function formatBytes(value: number) {
    if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
    return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  }

  function phaseLabel(phase: RuntimePhase) {
    const labels: Record<Locale, Record<RuntimePhase, string>> = {
      en: {
        needs_setup: "Setup required",
        idle: "Sync service running",
        syncing: "Syncing now",
        paused: "Sync paused",
        retry_waiting: "Retry scheduled",
        needs_attention: "Needs attention",
      },
      "zh-CN": {
        needs_setup: "需要完成设置",
        idle: "同步服务正在运行",
        syncing: "正在同步",
        paused: "同步已暂停",
        retry_waiting: "已安排重试",
        needs_attention: "需要处理",
      },
    };
    return labels[locale][phase];
  }

  function statusLabel(value: ActivityStatusValue) {
    const labels: Record<Locale, Record<ActivityStatusValue, string>> = {
      en: {
        accepted: "Synced",
        pending: "Waiting",
        submitting: "Syncing",
        retryable: "Retry scheduled",
        rejected: "Needs attention",
      },
      "zh-CN": {
        accepted: "同步成功",
        pending: "等待同步",
        submitting: "正在同步",
        retryable: "等待重试",
        rejected: "需要处理",
      },
    };
    return labels[locale][value];
  }
</script>

<main class:detail-open={selectedActivity !== null}>
  <header class="app-header">
    <button
      class="brand"
      type="button"
      onclick={() => openView("dashboard")}
      aria-label="Scrobble Bridge"
    >
      <span class="brand-mark">SB</span>
      <span><strong>Scrobble Bridge</strong><small>{text.runtime}</small></span>
    </button>
    {#if status}
      <div
        class:attention={status.phase === "needs_attention"}
        class="runtime-state"
      >
        {#if status.phase === "needs_attention"}
          <WarningCircle size={18} weight="fill" aria-hidden="true" />
        {:else if status.phase === "syncing"}
          <ArrowsClockwise size={18} aria-hidden="true" />
        {:else}
          <CheckCircle size={18} weight="fill" aria-hidden="true" />
        {/if}
        <span
          ><strong>{phaseLabel(status.phase)}</strong><small
            >{runtimeDetail(status)}</small
          ></span
        >
      </div>
    {/if}
    <div class="header-actions">
      {#if view !== "dashboard"}
        <button
          class="secondary-button"
          type="button"
          onclick={() => openView("dashboard")}
          ><ArrowLeft size={18} />{text.back}</button
        >
      {/if}
      <button
        class="primary-button"
        type="button"
        disabled={busy || !status?.configured || status?.paused}
        onclick={runSync}
      >
        <ArrowsClockwise size={18} />{busy ? text.syncing : text.syncNow}
      </button>
      {#if view !== "settings"}
        <button
          class="icon-button"
          type="button"
          title={text.settings}
          aria-label={text.settings}
          onclick={() => openView("settings")}><Gear size={21} /></button
        >
      {/if}
    </div>
  </header>

  {#if view === "dashboard"}
    <section class="dashboard page-shell">
      {#if softwareUpdate?.available_version && softwareUpdate.available_version !== dismissedUpdateVersion}
        <section class="update-banner" aria-live="polite">
          <span class="update-icon"
            ><DownloadSimple size={24} weight="bold" /></span
          >
          <span class="update-copy">
            <small>{text.updateAvailable}</small>
            <strong
              >{text.updateAvailableBody.replace(
                "{version}",
                softwareUpdate.available_version,
              )}</strong
            >
            {#if softwareUpdate.notes}
              <details>
                <summary>{text.releaseNotes}</summary>
                <p>{softwareUpdate.notes}</p>
              </details>
            {/if}
            {#if softwareUpdate.phase === "downloading"}
              <progress
                max={softwareUpdate.total_bytes ?? 1}
                value={softwareUpdate.downloaded_bytes}
              ></progress>
              <small
                >{formatBytes(
                  softwareUpdate.downloaded_bytes,
                )}{#if softwareUpdate.total_bytes}
                  / {formatBytes(softwareUpdate.total_bytes)}
                {/if}</small
              >
            {:else if softwareUpdate.phase === "ready"}
              <small class="update-verified"
                ><CheckCircle
                  size={14}
                  weight="fill"
                />{text.updateReady}</small
              >
            {/if}
          </span>
          <span class="update-actions">
            {#if softwareUpdate.phase === "ready"}
              <button
                class="primary-button"
                type="button"
                disabled={updateBusy}
                onclick={installUpdate}
                ><ArrowsClockwise size={17} />{updateBusy
                  ? text.installingUpdate
                  : text.updateNowRestart}</button
              >
            {:else}
              <button
                class="primary-button"
                type="button"
                disabled={updateBusy ||
                  softwareUpdate.phase === "downloading" ||
                  softwareUpdate.phase === "installing" ||
                  softwareUpdate.phase === "checking"}
                onclick={downloadUpdate}
                ><DownloadSimple size={17} />{softwareUpdate.phase ===
                "downloading"
                  ? text.downloadingUpdate
                  : softwareUpdate.phase === "installing"
                    ? text.installingUpdate
                    : text.downloadUpdate}</button
              >
            {/if}
            {#if softwareUpdate.phase !== "installing"}
              <button
                class="secondary-button"
                type="button"
                disabled={softwareUpdate.phase === "downloading"}
                onclick={() =>
                  (dismissedUpdateVersion =
                    softwareUpdate?.available_version ?? null)}
                >{text.later}</button
              >
            {/if}
          </span>
        </section>
      {/if}
      <section class="connection-card" aria-label={text.connections}>
        <div
          class:disconnected={!status?.ytmusic_configured}
          class="provider-summary"
        >
          <span class="provider-icon youtube"><SiYoutubemusic size={28} /></span
          >
          <span class="provider-copy"
            ><small>{text.source}</small><strong
              >{status?.ytmusic_account_name ?? text.accountUnavailable}</strong
            ><span
              >{status?.ytmusic_channel_handle ??
                (status?.ytmusic_configured
                  ? text.connected
                  : text.notConnected)}</span
            ></span
          >
          <span
            class:connected={status?.ytmusic_configured}
            class="connection-pill"
            >{status?.ytmusic_configured
              ? text.connected
              : text.notConnected}</span
          >
        </div>
        <div class="bridge-arrow">
          <span></span><ArrowRight size={22} /><span></span>
        </div>
        <div
          class:disconnected={!status?.lastfm_authorized}
          class="provider-summary"
        >
          <span class="provider-icon lastfm"><SiLastdotfm size={30} /></span>
          <span class="provider-copy"
            ><small>{text.destination}</small><strong
              >{status?.lastfm_username ?? text.notConnected}</strong
            ><span
              >{status?.lastfm_authorized
                ? text.connected
                : text.openSettings}</span
            ></span
          >
          <span
            class:connected={status?.lastfm_authorized}
            class="connection-pill"
            >{status?.lastfm_authorized
              ? text.connected
              : text.notConnected}</span
          >
        </div>
      </section>

      <section class="activity-section">
        <div class="section-heading">
          <span
            ><h1>{text.recentActivity}</h1>
            <p>{text.recentActivityBody}</p></span
          >
          <button
            class="text-button"
            type="button"
            onclick={() => openView("history")}
            >{text.allHistory}<CaretRight size={16} /></button
          >
        </div>
        {#if recent.items.length > 0}
          <ActivityList
            entries={recent.items}
            {locale}
            onSelect={(entry) => (selectedActivity = entry)}
          />
        {:else}
          <div class="empty-state">
            <span><Clock size={28} weight="duotone" /></span>
            <strong>{text.emptyTitle}</strong>
            <p>{text.emptyBody}</p>
            {#if status?.configured}<button
                class="secondary-button"
                disabled={busy || status.paused}
                type="button"
                onclick={runSync}
                ><ArrowsClockwise size={18} />{text.syncNow}</button
              >{/if}
          </div>
        {/if}
      </section>
    </section>
  {:else if view === "history"}
    <section class="history-view page-shell">
      <div class="page-title">
        <span
          ><h1>{text.history}</h1>
          <p>{text.historyBody}</p></span
        ><strong>{history.total.toLocaleString(dateLocale(locale))}</strong>
      </div>
      <form
        class="history-toolbar"
        onsubmit={(event) => {
          event.preventDefault();
          void applyHistoryFilters();
        }}
      >
        <label class="search-field"
          ><MagnifyingGlass size={18} /><input
            bind:value={historySearch}
            placeholder={text.search}
            aria-label={text.search}
          /></label
        >
        <select
          bind:value={historyStatus}
          onchange={() => applyHistoryFilters()}
          aria-label={text.allStatuses}
        >
          {#each STATUS_OPTIONS as option}<option value={option}
              >{option ? statusLabel(option) : text.allStatuses}</option
            >{/each}
        </select>
        <button class="secondary-button" type="submit">{text.search}</button>
      </form>
      {#if history.items.length > 0}
        <ActivityList
          entries={history.items}
          {locale}
          onSelect={(entry) => (selectedActivity = entry)}
        />
      {:else}
        <div class="empty-state compact">
          <MagnifyingGlass size={28} /><strong>{text.noResults}</strong>
        </div>
      {/if}
      <nav class="pagination" aria-label={text.history}>
        <span
          >{text.showing}
          {history.total === 0 ? 0 : history.offset + 1}–{Math.min(
            history.offset + history.items.length,
            history.total,
          )}
          {text.of}
          {history.total}</span
        >
        <span class="page-number"
          >{text.page} {Math.floor(history.offset / 50) + 1}</span
        >
        <button
          class="secondary-button"
          type="button"
          disabled={history.offset === 0}
          onclick={() => changePage(-1)}
          ><CaretLeft size={17} />{text.previous}</button
        >
        <button
          class="secondary-button"
          type="button"
          disabled={history.offset + history.items.length >= history.total}
          onclick={() => changePage(1)}
          >{text.next}<CaretRight size={17} /></button
        >
      </nav>
    </section>
  {:else}
    <section class="settings-view page-shell">
      <div class="page-title">
        <span
          ><h1>{text.settingsTitle}</h1>
          <p>{text.settingsBody}</p></span
        >
      </div>

      <section class="settings-section">
        <h2>{text.connections}</h2>
        <div class="settings-card provider-setting">
          <span class="provider-icon youtube"><SiYoutubemusic size={28} /></span
          >
          <span class="setting-copy"
            ><strong>{text.source}</strong><small
              >{status?.ytmusic_account_name ??
                (status?.ytmusic_configured
                  ? text.accountUnavailable
                  : text.notConnected)}</small
            >
            <p>{text.extensionManaged}</p></span
          >
          <span
            class:connected={status?.ytmusic_configured}
            class="connection-pill"
            >{status?.ytmusic_configured
              ? text.connected
              : text.notConnected}</span
          >
          {#if status?.ytmusic_configured}<button
              class="secondary-button"
              type="button"
              disabled={identityBusy}
              onclick={() => refreshIdentity()}
              ><ArrowsClockwise size={17} />{identityBusy
                ? text.refreshing
                : text.refreshAccount}</button
            >{/if}
        </div>
        <div class="settings-card provider-setting lastfm-setting">
          <span class="provider-icon lastfm"><SiLastdotfm size={30} /></span>
          <span class="setting-copy"
            ><strong>{text.destination}</strong><small
              >{status?.lastfm_username ??
                (status?.lastfm_application_configured
                  ? text.lastFmApplicationReady
                  : text.notConnected)}</small
            ></span
          >
          <span
            class:connected={status?.lastfm_authorized}
            class="connection-pill"
            >{status?.lastfm_authorized
              ? text.connected
              : text.notConnected}</span
          >
          {#if status?.lastfm_authorized}
            <button
              class="secondary-button"
              type="button"
              disabled={busy}
              onclick={() => openLastFmAuthorization()}
              ><Browser size={17} />{text.lastFmReauthorize}</button
            >
          {:else if status?.lastfm_application_configured}
            <div class="connection-actions">
              <button
                class="primary-button"
                type="button"
                disabled={busy}
                onclick={() => openLastFmAuthorization()}
                ><Browser size={17} />{text.lastFmAuthorize}</button
              >
              {#if authorizationStarted}<button
                  class="secondary-button"
                  type="button"
                  disabled={busy}
                  onclick={() => finishLastFm()}
                  ><CheckCircle size={17} />{text.lastFmFinish}</button
                >{/if}
              <button
                class="secondary-button"
                type="button"
                disabled={busy}
                onclick={() => (showLastFmForm = !showLastFmForm)}
                ><Key size={17} />{text.changeApplication}<CaretDown
                  class={showLastFmForm ? "rotated" : ""}
                  size={16}
                /></button
              >
            </div>
          {:else}
            <button
              class="secondary-button"
              type="button"
              onclick={() => (showLastFmForm = !showLastFmForm)}
              ><Key size={17} />{text.configureApplication}<CaretDown
                class={showLastFmForm ? "rotated" : ""}
                size={16}
              /></button
            >
          {/if}
          {#if showLastFmForm}
            <form
              class="credential-form"
              onsubmit={(event) => {
                event.preventDefault();
                void openLastFmAuthorization(true);
              }}
            >
              <label
                >{text.apiKey}<input
                  required
                  autocomplete="off"
                  bind:value={apiKey}
                /></label
              >
              <label
                >{text.sharedSecret}<input
                  required
                  type="password"
                  autocomplete="off"
                  bind:value={sharedSecret}
                /></label
              >
              <button
                class="primary-button"
                type="submit"
                disabled={busy || !apiKey.trim() || !sharedSecret.trim()}
                >{text.saveAndAuthorize}<ArrowRight size={17} /></button
              >
            </form>
          {/if}
        </div>
      </section>

      <section class="settings-section">
        <h2>{text.preferences}</h2>
        <div class="settings-card setting-list">
          <div class="setting-row update-setting">
            <span class="row-icon"><DownloadSimple size={21} /></span><span
              class="setting-copy"
              ><strong>{text.softwareUpdates}</strong>
              <p>
                {text.currentVersion}: {softwareUpdate?.current_version ?? "—"} ·
                {text.dailyUpdateBody}
              </p>
              <small
                >{text.lastUpdateCheck}: {formatDate(
                  softwareUpdate?.last_checked_at,
                )} · {text.nextUpdateCheck}: {formatDate(
                  softwareUpdate?.next_check_at,
                )}</small
              >
              {#if softwareUpdate?.error}<small class="setting-error"
                  >{text.updateCheckFailed} {softwareUpdate.error}</small
                >{/if}
            </span>
            <span class="setting-actions">
              {#if softwareUpdate?.phase === "ready"}
                <button
                  class="primary-button"
                  type="button"
                  disabled={updateBusy}
                  onclick={installUpdate}
                  >{updateBusy
                    ? text.installingUpdate
                    : text.updateNowRestart}</button
                >
              {:else if softwareUpdate?.available_version}
                <button
                  class="primary-button"
                  type="button"
                  disabled={updateBusy ||
                    softwareUpdate.phase === "downloading"}
                  onclick={downloadUpdate}
                  >{softwareUpdate.phase === "downloading"
                    ? text.downloadingUpdate
                    : text.downloadUpdate}</button
                >
              {:else}
                <button
                  class="secondary-button"
                  type="button"
                  disabled={updateBusy || softwareUpdate?.phase === "checking"}
                  onclick={checkForUpdate}
                  >{updateBusy || softwareUpdate?.phase === "checking"
                    ? text.checkingUpdates
                    : text.checkForUpdates}</button
                >
              {/if}
            </span>
          </div>
          <div class="setting-row">
            <span class="row-icon"><Browser size={21} /></span><span
              class="setting-copy"
              ><strong>{text.language}</strong>
              <p>{text.languageBody}</p></span
            ><select
              value={locale}
              onchange={(event) =>
                selectLocale(event.currentTarget.value as Locale)}
              ><option value="zh-CN">简体中文</option><option value="en"
                >English</option
              ></select
            >
          </div>
          <div class="setting-row">
            <span class="row-icon"><Play size={21} /></span><span
              class="setting-copy"
              ><strong>{text.launchAtLogin}</strong>
              <p>{text.launchAtLoginBody}</p></span
            ><button
              class:active={launchAtLogin}
              class="switch"
              type="button"
              role="switch"
              aria-checked={launchAtLogin}
              aria-label={text.launchAtLogin}
              onclick={toggleAutostart}><span></span></button
            >
          </div>
          <div class="setting-row">
            <span class="row-icon"
              >{#if status?.paused}<Pause size={21} />{:else}<CheckCircle
                  size={21}
                />{/if}</span
            ><span class="setting-copy"
              ><strong>{text.syncService}</strong>
              <p>{status?.paused ? text.paused : text.active}</p></span
            ><button
              class="secondary-button"
              type="button"
              disabled={busy}
              onclick={() =>
                run(
                  status?.paused ? desktop.resume : desktop.pause,
                  status?.paused ? text.resumedMessage : text.pausedMessage,
                )}
              >{#if status?.paused}<Play size={17} />{text.resume}{:else}<Pause
                  size={17}
                />{text.pause}{/if}</button
            >
          </div>
          <div class="setting-row">
            <span class="row-icon"><Export size={21} /></span><span
              class="setting-copy"
              ><strong>{text.diagnostics}</strong>
              <p>{text.diagnosticsBody}</p></span
            ><button
              class="secondary-button"
              type="button"
              disabled={busy}
              onclick={exportDiagnostics}
              ><Export size={17} />{text.export}</button
            >
          </div>
        </div>
      </section>

      <section class="settings-section advanced-section">
        <button
          class="advanced-toggle"
          type="button"
          aria-expanded={showAdvanced}
          onclick={() => (showAdvanced = !showAdvanced)}
          ><span
            ><strong>{text.advanced}</strong><small>{text.advancedBody}</small
            ></span
          ><CaretDown class={showAdvanced ? "rotated" : ""} size={19} /></button
        >
        {#if showAdvanced}
          <form
            class="settings-card credential-form advanced-form"
            onsubmit={(event) => {
              event.preventDefault();
              void run(
                () =>
                  desktop.saveYouTubeMusic(accountId, authUser, cookieHeader),
                text.manualSaved,
              );
            }}
          >
            <label
              >{text.accountId}<input required bind:value={accountId} /></label
            >
            <label
              >{text.authUser}<input
                required
                min="0"
                type="number"
                bind:value={authUser}
              /></label
            >
            <label class="wide"
              >{text.cookie}<textarea
                required
                rows="4"
                autocomplete="off"
                bind:value={cookieHeader}
              ></textarea></label
            >
            <button
              class="primary-button"
              type="submit"
              disabled={busy || !cookieHeader.trim()}>{text.saveYouTube}</button
            >
          </form>
        {/if}
      </section>
    </section>
  {/if}

  <footer>
    <span class:running={!status?.paused}></span>{text.background}
  </footer>
</main>

{#if selectedActivity}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) selectedActivity = null;
    }}
  >
    <div
      class="detail-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="detail-title"
    >
      <header>
        <span
          ><small>{text.activityDetail}</small>
          <h2 id="detail-title">{selectedActivity.candidate.track.title}</h2>
          <p>{selectedActivity.candidate.track.artist}</p></span
        ><button
          class="icon-button"
          type="button"
          aria-label={text.close}
          onclick={() => (selectedActivity = null)}><X size={20} /></button
        >
      </header>
      <div class="detail-result">
        <ActivityStatus status={selectedActivity.status} {locale} />
      </div>
      <dl>
        <div>
          <dt>{text.listened}</dt>
          <dd>{formatDate(selectedActivity.candidate.started_at)}</dd>
        </div>
        <div>
          <dt>{text.discovered}</dt>
          <dd>{formatDate(selectedActivity.created_at)}</dd>
        </div>
        <div>
          <dt>{text.lastUpdated}</dt>
          <dd>{formatDate(selectedActivity.updated_at)}</dd>
        </div>
        <div>
          <dt>{text.attempts}</dt>
          <dd>{selectedActivity.attempt_count}</dd>
        </div>
        <div>
          <dt>{text.sourceTrack}</dt>
          <dd>{text.source}</dd>
        </div>
        <div>
          <dt>{text.destinationState}</dt>
          <dd>{statusLabel(selectedActivity.status)}</dd>
        </div>
        <div class="wide">
          <dt>{text.error}</dt>
          <dd>{selectedActivity.last_error_code ?? text.noError}</dd>
        </div>
      </dl>
      <button
        class="secondary-button modal-close"
        type="button"
        onclick={() => (selectedActivity = null)}>{text.close}</button
      >
    </div>
  </div>
{/if}

{#if message || errorMessage}
  <div class:error={Boolean(errorMessage)} class="toast" role="status">
    {#if errorMessage}<WarningCircle
        size={18}
        weight="fill"
      />{errorMessage}{:else}<CheckCircle
        size={18}
        weight="fill"
      />{message}{/if}
  </div>
{/if}
