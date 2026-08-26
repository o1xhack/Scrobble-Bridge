import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktop } from "./transport";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("desktop command transport", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("uses the expected Rust command and camel-case credential arguments", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await desktop.saveYouTubeMusic("account", 2, "cookie");

    expect(invoke).toHaveBeenCalledWith("save_ytmusic_credentials", {
      accountId: "account",
      authUser: 2,
      delegatedSessionId: null,
      cookieHeader: "cookie",
    });
  });

  it("maps pause and resume to separate commands", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await desktop.pause();
    await desktop.resume();

    expect(invoke).toHaveBeenNthCalledWith(1, "pause_sync");
    expect(invoke).toHaveBeenNthCalledWith(2, "resume_sync");
  });

  it("requests bounded activity pages with nullable filters", async () => {
    vi.mocked(invoke).mockResolvedValue({
      items: [],
      total: 0,
      limit: 50,
      offset: 100,
    });

    await desktop.activity(50, 100, "Radiohead", "accepted");
    await desktop.activity();

    expect(invoke).toHaveBeenNthCalledWith(1, "activity", {
      limit: 50,
      offset: 100,
      search: "Radiohead",
      status: "accepted",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "activity", {
      limit: 50,
      offset: 0,
      search: null,
      status: null,
    });
  });

  it("refreshes YouTube Music identity without exposing credentials", async () => {
    vi.mocked(invoke).mockResolvedValue({
      account_name: "Listener",
      channel_handle: "@listener",
      photo_url: null,
    });

    await desktop.refreshYouTubeMusicIdentity();

    expect(invoke).toHaveBeenCalledWith("refresh_ytmusic_identity");
  });

  it("keeps update download and installation as separate user actions", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await desktop.checkForUpdate();
    await desktop.downloadUpdate();
    await desktop.installUpdate();

    expect(invoke).toHaveBeenNthCalledWith(1, "check_for_software_update");
    expect(invoke).toHaveBeenNthCalledWith(2, "download_software_update");
    expect(invoke).toHaveBeenNthCalledWith(3, "install_software_update");
  });
});
