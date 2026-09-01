<script lang="ts">
  import {
    classifySyncReport,
    dateLocale,
    LOCALE_STORAGE_KEY,
    resolveLocale,
    StatusBadge,
    type Locale,
    type RuntimeStatus,
    type SyncReport,
  } from "@scrobble-bridge/ui";
  import { onMount } from "svelte";
  import { ApiClient, type PairedDevice, type PairingRequest } from "./api";

  const COPY = {
    en: {
      language: "Language",
      tagline: "Local-first music history sync",
      disconnect: "Lock",
      eyebrow: "YOUR MUSIC · YOUR MACHINE",
      hero1: "Keep listening.",
      hero2: "We’ll keep the record.",
      lede: "Scrobble YouTube Music plays to Last.fm from your desktop or NAS. No hosted account, no credential relay, no subscription.",
      firstRun: "First-run access",
      unlockTitle: "Unlock this NAS runtime",
      unlockBody:
        "Paste the admin token from /data/secrets/admin.token. It stays in this browser tab only.",
      adminToken: "Admin token",
      unlock: "Unlock",
      syncStatus: "Sync status",
      connected: "Bridge is connected",
      finishSetup: "Finish setup to begin",
      lastSuccess: "Last success",
      pending: "Pending",
      retrying: "Retrying",
      syncNow: "Sync now",
      syncBaseline: "History baseline saved. New plays will sync from now on.",
      syncNoChanges:
        "Checked successfully. No new listening activity was found.",
      syncAccepted: "Synced {count} track(s) to Last.fm.",
      syncGap:
        "YouTube Music history could not be safely aligned. Nothing new was sent to Last.fm; automatic retries will continue.",
      syncIncomplete:
        "Some tracks were not accepted by Last.fm. They remain queued for retry or need attention.",
      resume: "Resume",
      pause: "Pause",
      resumed: "Synchronization resumed.",
      paused: "Synchronization paused.",
      source: "01 · YouTube Music",
      sourceBody:
        "The Chrome extension normally supplies this snapshot. Manual entry is available for NAS recovery.",
      accountLabel: "Account label",
      googleIndex: "Google account index",
      cookieHeader: "Cookie header",
      saveSource: "Save YouTube Music",
      sourceSaved: "YouTube Music credentials saved.",
      destination: "02 · Last.fm",
      lastFmBody:
        "Authorize your own Last.fm account once. The shared secret is encrypted before it reaches disk.",
      apiKey: "API key",
      sharedSecret: "Shared secret",
      authorizeLastFm: "Open Last.fm authorization",
      approved: "I approved access",
      lastFmConnected: "Last.fm account connected.",
      popups: "Allow pop-ups for this page, then try again.",
      authOpened: "Last.fm authorization opened in a new tab.",
      browserConnection: "03 · Browser connection",
      pairingTitle: "Chrome extension pairing",
      pairingBody:
        "Create a one-time code, then enter this server’s HTTPS address and the code in the extension. Codes expire after ten minutes.",
      deviceLabel: "Device label",
      pairingDefault: "Chrome on this computer",
      createCode: "Create pairing code",
      pairingCreated:
        "Pairing code created. Enter it in the Chrome extension before it expires.",
      expires: "Expires",
      pairedDevices: "Paired devices",
      noDevices: "No browser has been paired yet.",
      added: "Added",
      revoke: "Revoke",
      revoked: "Device access revoked.",
      daemonConnected: "Connected to the daemon.",
      unofficial: "Unofficial and independent.",
      credentialsStay: "Credentials stay on your devices.",
      playEstimates: "Play times are estimates.",
      poweredBy: "Powered by Last.fm",
      notYet: "Not yet",
      description: "Private, local-first YouTube Music to Last.fm scrobbling.",
    },
    "zh-CN": {
      language: "语言",
      tagline: "本地优先的音乐记录同步",
      disconnect: "锁定",
      eyebrow: "你的音乐 · 你的设备",
      hero1: "继续听音乐，",
      hero2: "记录交给我们。",
      lede: "从桌面电脑或 NAS 将 YouTube Music 播放记录同步到 Last.fm。无需托管账号，不中转凭据，也没有订阅费。",
      firstRun: "首次访问",
      unlockTitle: "解锁这台 NAS 服务",
      unlockBody:
        "请粘贴 /data/secrets/admin.token 中的管理员令牌。它只保留在当前浏览器标签页。",
      adminToken: "管理员令牌",
      unlock: "解锁",
      syncStatus: "同步状态",
      connected: "同步桥已连接",
      finishSetup: "完成设置后即可开始",
      lastSuccess: "上次成功",
      pending: "待处理",
      retrying: "等待重试",
      syncNow: "立即同步",
      syncBaseline: "已保存历史基准；之后出现的新播放会自动同步。",
      syncNoChanges: "检查正常，没有发现新的收听记录。",
      syncAccepted: "已成功同步 {count} 首歌曲到 Last.fm。",
      syncGap:
        "YouTube Music 历史无法安全衔接，本轮没有向 Last.fm 提交新记录；程序会继续自动重试。",
      syncIncomplete: "部分歌曲尚未被 Last.fm 接受，已保留等待重试或需要处理。",
      resume: "继续",
      pause: "暂停",
      resumed: "已继续同步。",
      paused: "已暂停同步。",
      source: "01 · YouTube Music",
      sourceBody:
        "通常由 Chrome 扩展自动提供登录凭据。手动填写仅用于 NAS 恢复。",
      accountLabel: "账号标签",
      googleIndex: "Google 账号序号",
      cookieHeader: "Cookie 请求头",
      saveSource: "保存 YouTube Music",
      sourceSaved: "YouTube Music 凭据已保存。",
      destination: "02 · Last.fm",
      lastFmBody:
        "只需授权一次你自己的 Last.fm 账号。Shared Secret 写入磁盘前会加密。",
      apiKey: "API Key",
      sharedSecret: "Shared Secret",
      authorizeLastFm: "打开 Last.fm 授权",
      approved: "我已批准访问",
      lastFmConnected: "Last.fm 账号已连接。",
      popups: "请允许此页面打开弹窗，然后重试。",
      authOpened: "Last.fm 授权已在新标签页打开。",
      browserConnection: "03 · 浏览器连接",
      pairingTitle: "Chrome 扩展配对",
      pairingBody:
        "创建一次性配对码，然后在扩展中输入这台服务的 HTTPS 地址和配对码。配对码十分钟后过期。",
      deviceLabel: "设备标签",
      pairingDefault: "这台电脑上的 Chrome",
      createCode: "创建配对码",
      pairingCreated: "配对码已创建，请在过期前输入 Chrome 扩展。",
      expires: "过期时间",
      pairedDevices: "已配对设备",
      noDevices: "还没有浏览器完成配对。",
      added: "添加时间",
      revoke: "撤销",
      revoked: "设备访问权限已撤销。",
      daemonConnected: "已连接到 NAS 同步服务。",
      unofficial: "非官方独立项目。",
      credentialsStay: "凭据始终保存在你的设备上。",
      playEstimates: "播放时间为估算值。",
      poweredBy: "由 Last.fm 提供支持",
      notYet: "尚未同步",
      description: "本地优先的 YouTube Music 到 Last.fm 同步服务。",
    },
  } as const;

  let locale = $state<Locale>("en");
  let text = $derived(COPY[locale]);
  let token = $state(sessionStorage.getItem("scrobble-admin-token") ?? "");
  let client = $derived(token ? new ApiClient(token) : null);
  let status = $state<RuntimeStatus | null>(null);
  let busy = $state(false);
  let error = $state("");
  let notice = $state("");
  let accountId = $state("default");
  let authUser = $state(0);
  let cookieHeader = $state("");
  let apiKey = $state("");
  let sharedSecret = $state("");
  let pairingLabel = $state<string>(COPY.en.pairingDefault);
  let pairing = $state<PairingRequest | null>(null);
  let devices = $state<PairedDevice[]>([]);

  onMount(() => {
    selectLocale(
      resolveLocale(
        localStorage.getItem(LOCALE_STORAGE_KEY),
        navigator.language,
      ),
      false,
    );
    if (client) void refresh();
    const timer = window.setInterval(
      () => client && void refresh(true),
      15_000,
    );
    return () => window.clearInterval(timer);
  });

  function selectLocale(next: Locale, persist = true) {
    const oldDefault = COPY[locale].pairingDefault;
    locale = next;
    if (pairingLabel === oldDefault) pairingLabel = COPY[next].pairingDefault;
    document.documentElement.lang = next;
    if (persist) localStorage.setItem(LOCALE_STORAGE_KEY, next);
  }

  async function run(action: () => Promise<unknown>, success: string) {
    busy = true;
    error = "";
    notice = "";
    try {
      await action();
      notice = success;
      await refresh(true);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
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
    if (!client) return;
    busy = true;
    error = "";
    notice = "";
    try {
      const feedback = syncFeedbackText(await client.sync());
      if (feedback.error) error = feedback.value;
      else notice = feedback.value;
      await refresh(true);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  function runtimeErrorText(current: RuntimeStatus) {
    if (current.last_error_code === "history_gap") return text.syncGap;
    if (current.last_error_code === "submission_incomplete")
      return text.syncIncomplete;
    return current.last_error_message;
  }

  async function refresh(silent = false) {
    if (!client) return;
    try {
      status = await client.status();
      devices = await client.devices();
      if (!silent) notice = text.daemonConnected;
      error = "";
    } catch (cause) {
      if (!silent)
        error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function createPairingCode() {
    if (!client) return;
    busy = true;
    error = "";
    notice = "";
    try {
      pairing = await client.startPairing(pairingLabel);
      notice = text.pairingCreated;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function authorizeLastFm() {
    if (!client) return;
    const authorizationWindow = window.open("about:blank", "_blank");
    if (authorizationWindow) authorizationWindow.opener = null;
    busy = true;
    error = "";
    notice = "";
    try {
      await client.saveLastFmApplication({
        api_key: apiKey,
        shared_secret: sharedSecret,
      });
      const auth = await client.startLastFm();
      if (!authorizationWindow) throw new Error(text.popups);
      authorizationWindow.location.replace(auth.authorization_url);
      notice = text.authOpened;
    } catch (cause) {
      authorizationWindow?.close();
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  function connect() {
    token = token.trim();
    sessionStorage.setItem("scrobble-admin-token", token);
    void refresh();
  }

  function disconnect() {
    sessionStorage.removeItem("scrobble-admin-token");
    token = "";
    status = null;
    devices = [];
    pairing = null;
    notice = "";
    error = "";
  }

  function formatDate(value: string | null): string {
    return value
      ? new Intl.DateTimeFormat(dateLocale(locale), {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(new Date(value))
      : text.notYet;
  }
</script>

<svelte:head><meta name="description" content={text.description} /></svelte:head
>

<header>
  <a class="brand" href="/" aria-label="Scrobble Bridge">
    <span aria-hidden="true">
      <svg viewBox="0 0 36 36">
        <path
          d="M4 18c3.1-7.2 6.2-7.2 9.3 0s6.2 7.2 9.3 0 6.2-7.2 9.3 0"
          fill="none"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-width="4"
        />
      </svg>
    </span>
    Scrobble Bridge
  </a>
  <div class="header-tools">
    <label class="language"
      ><span>{text.language}</span><select
        value={locale}
        onchange={(event) => selectLocale(event.currentTarget.value as Locale)}
        ><option value="zh-CN">简体中文</option><option value="en"
          >English</option
        ></select
      ></label
    >
    {#if status}<button class="header-action" onclick={disconnect}
        >{text.disconnect}</button
      >{:else}<p>{text.tagline}</p>{/if}
  </div>
</header>

<main>
  <section class="hero">
    <div>
      <p class="eyebrow">{text.eyebrow}</p>
      <h1>{text.hero1}<br />{text.hero2}</h1>
      <p class="lede">{text.lede}</p>
    </div>
    <div class="signal" aria-hidden="true">
      <b></b><b></b><b></b><b></b><b></b><b></b><b></b>
    </div>
  </section>

  {#if !status}
    <section class="card login">
      <div>
        <p class="step">{text.firstRun}</p>
        <h2>{text.unlockTitle}</h2>
        <p>{text.unlockBody}</p>
      </div>
      <form
        onsubmit={(event) => {
          event.preventDefault();
          connect();
        }}
      >
        <label
          >{text.adminToken}<input
            bind:value={token}
            type="password"
            autocomplete="off"
            required
          /></label
        ><button disabled={!token.trim() || busy}>{text.unlock}</button>
      </form>
    </section>
  {:else}
    <section class="overview card">
      <div>
        <p class="step">{text.syncStatus}</p>
        <h2>{status.configured ? text.connected : text.finishSetup}</h2>
        <StatusBadge phase={status.phase} {locale} />
      </div>
      <div class="metrics">
        <article>
          <small>{text.lastSuccess}</small><strong
            >{formatDate(status.last_success_at)}</strong
          >
        </article>
        <article>
          <small>{text.pending}</small><strong>{status.pending}</strong>
        </article>
        <article>
          <small>{text.retrying}</small><strong>{status.retryable}</strong>
        </article>
      </div>
      <div class="actions">
        <button
          class="secondary"
          disabled={busy || !status.configured}
          onclick={runSync}>{text.syncNow}</button
        ><button
          class="ghost"
          disabled={busy}
          onclick={() =>
            client &&
            run(
              () => (status?.paused ? client.resume() : client.pause()),
              status?.paused ? text.resumed : text.paused,
            )}>{status.paused ? text.resume : text.pause}</button
        >
      </div>
    </section>

    {#if status.last_error_message}<div class="alert error">
        {runtimeErrorText(status)}
      </div>{/if}

    <section class="setup-grid">
      <form
        class="card"
        onsubmit={(event) => {
          event.preventDefault();
          if (client)
            void run(
              () =>
                client.saveYouTubeMusic({
                  account_id: accountId,
                  auth_user: authUser,
                  cookie_header: cookieHeader,
                }),
              text.sourceSaved,
            );
        }}
      >
        <p class="step">{text.source}</p>
        <h2>YouTube Music</h2>
        <p>{text.sourceBody}</p>
        <label
          >{text.accountLabel}<input
            bind:value={accountId}
            maxlength="128"
            required
          /></label
        ><label
          >{text.googleIndex}<input
            bind:value={authUser}
            type="number"
            min="0"
            max="9"
            required
          /></label
        ><label
          >{text.cookieHeader}<textarea
            bind:value={cookieHeader}
            rows="4"
            spellcheck="false"
            required
          ></textarea></label
        ><button disabled={busy}>{text.saveSource}</button>
      </form>

      <form
        class="card"
        onsubmit={(event) => {
          event.preventDefault();
          void authorizeLastFm();
        }}
      >
        <p class="step">{text.destination}</p>
        <h2>Last.fm</h2>
        <p>{text.lastFmBody}</p>
        <label
          >{text.apiKey}<input
            bind:value={apiKey}
            maxlength="512"
            autocomplete="off"
            required
          /></label
        ><label
          >{text.sharedSecret}<input
            bind:value={sharedSecret}
            type="password"
            maxlength="512"
            autocomplete="off"
            required
          /></label
        ><button disabled={busy}>{text.authorizeLastFm}</button><button
          class="ghost full"
          type="button"
          disabled={busy}
          onclick={() =>
            client && run(() => client.finishLastFm(), text.lastFmConnected)}
          >{text.approved}</button
        >
      </form>
    </section>

    <section class="card devices">
      <div>
        <p class="step">{text.browserConnection}</p>
        <h2>{text.pairingTitle}</h2>
        <p>{text.pairingBody}</p>
        <form
          class="pairing-form"
          onsubmit={(event) => {
            event.preventDefault();
            void createPairingCode();
          }}
        >
          <label
            >{text.deviceLabel}<input
              bind:value={pairingLabel}
              maxlength="128"
              required
            /></label
          ><button disabled={busy || !pairingLabel.trim()}
            >{text.createCode}</button
          >
        </form>
        {#if pairing}<div class="pairing-code" aria-live="polite">
            <code>{pairing.code}</code><small
              >{text.expires} {formatDate(pairing.expires_at)}</small
            >
          </div>{/if}
      </div>
      <div>
        <h3>{text.pairedDevices}</h3>
        {#if devices.length === 0}<p>{text.noDevices}</p>{:else}<ul
            class="device-list"
          >
            {#each devices as device (device.id)}<li>
                <span
                  ><strong>{device.label}</strong><small
                    >{text.added} {formatDate(device.created_at)}</small
                  ></span
                ><button
                  class="ghost"
                  disabled={busy}
                  onclick={() =>
                    client &&
                    run(() => client.revokeDevice(device.id), text.revoked)}
                  >{text.revoke}</button
                >
              </li>{/each}
          </ul>{/if}
      </div>
    </section>
  {/if}

  {#if notice}<div class="toast" role="status">{notice}</div>{/if}
  {#if error}<div class="toast failure" role="alert">{error}</div>{/if}

  <footer>
    <span>{text.unofficial}</span><span>{text.credentialsStay}</span><span
      >{text.playEstimates}</span
    ><a href="https://www.last.fm/" target="_blank" rel="noreferrer"
      >{text.poweredBy}</a
    >
  </footer>
</main>
