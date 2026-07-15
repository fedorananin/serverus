import { describe, expect, it } from "vitest";

import { joinPath, parentPath } from "./format";

describe("file pane paths", () => {
  it("navigates to POSIX parent directories", () => {
    expect(parentPath("/srv/serverus/local-source")).toBe("/srv/serverus");
    expect(parentPath("/srv")).toBe("/");
    expect(parentPath("/")).toBe("/");
  });

  it("navigates to Windows parent directories without leaving the drive", () => {
    expect(parentPath("C:\\serverus\\local-source")).toBe("C:\\serverus");
    expect(parentPath("C:\\serverus")).toBe("C:\\");
    expect(parentPath("C:\\")).toBe("C:\\");
  });

  it("does not navigate above a Windows UNC share", () => {
    expect(parentPath("\\\\server\\share\\local-source")).toBe("\\\\server\\share");
    expect(parentPath("\\\\server\\share")).toBe("\\\\server\\share");
  });

  it("joins local Windows and remote POSIX paths with their visible separator", () => {
    expect(joinPath("C:\\serverus", "folder")).toBe("C:\\serverus\\folder");
    expect(joinPath("C:\\", "folder")).toBe("C:\\folder");
    expect(joinPath("\\\\server\\share", "folder")).toBe("\\\\server\\share\\folder");
    expect(joinPath("/srv/serverus", "folder")).toBe("/srv/serverus/folder");
  });

  it("keeps backslashes inside POSIX remote names as ordinary characters", () => {
    expect(parentPath("/remote/folder\\name")).toBe("/remote");
    expect(joinPath("/remote/folder\\name", "child")).toBe("/remote/folder\\name/child");
  });
});
