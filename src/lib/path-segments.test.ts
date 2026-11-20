import { describe, expect, it } from "vitest";

import { parentPath } from "./format";
import { pathCrumbs } from "./path-segments";

const paths = (path: string) => pathCrumbs(path).segments.map((s) => s.path);
const labels = (path: string) => pathCrumbs(path).segments.map((s) => s.label);

describe("pathCrumbs", () => {
  it("splits POSIX paths below the root", () => {
    expect(labels("/var/www/site")).toEqual(["/", "var", "www", "site"]);
    expect(paths("/var/www/site")).toEqual(["/", "/var", "/var/www", "/var/www/site"]);
    expect(pathCrumbs("/var").separator).toBe("/");
  });

  it("shows the root alone for /", () => {
    expect(pathCrumbs("/").segments).toEqual([{ label: "/", path: "/" }]);
  });

  it("ignores trailing and doubled slashes", () => {
    expect(paths("/srv//data/")).toEqual(["/", "/srv", "/srv/data"]);
  });

  it("keeps S3 bucket/prefix paths as folders", () => {
    expect(paths("/my-bucket/assets/css")).toEqual([
      "/",
      "/my-bucket",
      "/my-bucket/assets",
      "/my-bucket/assets/css",
    ]);
  });

  it("keeps names with spaces and dots intact", () => {
    expect(labels("/home/me/My Files/v1.2")).toEqual(["/", "home", "me", "My Files", "v1.2"]);
  });

  it("splits Windows drive paths in either slash style", () => {
    expect(labels("C:\\Users\\me")).toEqual(["C:\\", "Users", "me"]);
    expect(paths("C:\\Users\\me")).toEqual(["C:\\", "C:\\Users", "C:\\Users\\me"]);
    expect(pathCrumbs("C:\\Users").separator).toBe("\\");
    expect(paths("D:/work/repo")).toEqual(["D:/", "D:/work", "D:/work/repo"]);
    expect(pathCrumbs("D:/work").separator).toBe("/");
  });

  it("treats a bare drive as its root", () => {
    expect(paths("C:")).toEqual(["C:\\"]);
    expect(paths("C:\\")).toEqual(["C:\\"]);
  });

  it("roots UNC paths at the share", () => {
    expect(labels("\\\\nas\\media\\films\\2026")).toEqual(["\\\\nas\\media", "films", "2026"]);
    expect(paths("\\\\nas\\media\\films")).toEqual(["\\\\nas\\media", "\\\\nas\\media\\films"]);
    expect(paths("\\\\nas\\media")).toEqual(["\\\\nas\\media"]);
  });

  it("splits relative paths without inventing a root", () => {
    expect(paths("home/user")).toEqual(["home", "home/user"]);
    expect(pathCrumbs("").segments).toEqual([]);
  });

  it("agrees with parentPath on every crumb's parent", () => {
    for (const path of ["/var/www/site", "C:\\Users\\me\\Desktop", "\\\\nas\\media\\films\\x"]) {
      const crumbs = paths(path);
      for (let i = crumbs.length - 1; i > 0; i--) {
        expect(parentPath(crumbs[i])).toBe(crumbs[i - 1]);
      }
    }
  });
});
