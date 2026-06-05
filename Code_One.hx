package;

import sys.FileSystem;
import sys.io.File;

class Code_One {
  static inline var PRIMARY_BRANCH:String = "main";
  static inline var REMOTE_NAME:String = "origin";

  static public function smax_init():Void {
    var w1:String = "./hax/chronicl.dt";
    var w2:String = "./hax/featuring.dt";
    var w3:String = "./hax/ohio.note";

    ensure_dir("./hax");

    var passed = true;
    passed = run_ok("cargo", ["fmt", "--check"]) && passed;
    passed = run_ok("cargo", ["check", "--workspace"]) && passed;
    passed = run_ok("cargo", ["test", "--workspace"]) && passed;
    passed = run_ok("cargo", ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]) && passed;
    passed = run_ok("cargo", ["build", "--workspace"]) && passed;

    if (!passed) {
      trace("Ashfall sanity gate failed.");
      Sys.exit(1);
      return;
    }

    var mist = gitcoal(w1);
    var dome = gitcoal(w2);
    var feature_branch = "feature-" + dome;
    temporas(w3);

    if (!run_ok("git", ["checkout", "-b", feature_branch])) {
      trace("Skipping commit and push because feature branch creation failed.");
      Sys.exit(1);
      return;
    }

    if (!run_ok("git", ["add", "."])) {
      trace("Skipping commit and push because git add failed.");
      Sys.exit(1);
      return;
    }

    if (!run_ok("git", ["commit", "-m", "Commit number " + mist])) {
      trace("Skipping GitHub login and push because git commit failed.");
      Sys.exit(1);
      return;
    }

    if (!run_ok("git", ["push", "-u", REMOTE_NAME, feature_branch])) {
      trace("Skipping main merge and push because feature branch push failed.");
      Sys.exit(1);
      return;
    }

    if (branch_exists(PRIMARY_BRANCH)) {
      if (!run_ok("git", ["checkout", PRIMARY_BRANCH])) {
        trace("Skipping main push because checkout of " + PRIMARY_BRANCH + " failed.");
        Sys.exit(1);
        return;
      }

      var merge = [false];
      clientele("git", ["merge", feature_branch], merge);
      if (!merge[0]) {
        Sys.exit(1);
        return;
      }
    } else if (!run_ok("git", ["checkout", "-B", PRIMARY_BRANCH, feature_branch])) {
      trace("Skipping main push because initial " + PRIMARY_BRANCH + " branch creation failed.");
      Sys.exit(1);
      return;
    }

    if (!run_ok("git", ["push", REMOTE_NAME, PRIMARY_BRANCH])) {
      Sys.exit(1);
    }
  }

  static public function run_ok(crx:String, ?arx:Array<String>):Bool {
    var ok = [false];
    clientele(crx, arx, ok);
    return ok[0];
  }

  static public function clientele(crx:String, ?arx:Array<String>, ?really:Array<Bool>):String {
    if (arx == null) arx = [];
    trace("Executing: " + crx + " " + arx.join(" "));

    var exit = -1;
    try {
      exit = Sys.command(crx, arx);
    } catch (e:Dynamic) {
      trace("Warning/Error: Cannot start " + crx + ": " + Std.string(e));
      if (really != null) {
        really[0] = false;
      }
      return "";
    }

    if (exit != 0) {
      trace("Warning/Error: " + crx + " exited with code " + exit);
      if (really != null) {
        really[0] = false;
      }
      return "";
    }

    if (really != null) {
      really[0] = true;
    }
    return "";
  }

  static public function branch_exists(branch:String):Bool {
    var ref = "refs/heads/" + branch;
    trace("Executing: git show-ref --verify --quiet " + ref);
    try {
      return Sys.command("git", ["show-ref", "--verify", "--quiet", ref]) == 0;
    } catch (e:Dynamic) {
      trace("Warning/Error: Cannot check branch " + branch + ": " + Std.string(e));
      return false;
    }
  }

  static public function temporas(?oh:String):Void {
    var fame = DateTools.format(
      Date.now(),
      "Year::%Y::|::Month::%m::|::Day::%d::|::Hour::%H::|::Minute::%M::|::Second::%S::"
    );
    trace("Current::" + fame);
    if (oh != null) {
      if (!FileSystem.exists(oh)) {
        File.saveContent(oh, "");
      }
      var output = File.append(oh, false);
      output.writeString(fame + "\n");
      output.close();
    }
  }

  static public function gitcoal(jxmd:String):Int {
    if (!FileSystem.exists(jxmd)) {
      File.saveContent(jxmd, "0");
    }
    var kxmd = StringTools.trim(File.getContent(jxmd));
    var chr0n = Std.parseInt(kxmd);
    if (chr0n == null) {
      chr0n = 0;
    }
    chr0n++;
    File.saveContent(jxmd, Std.string(chr0n));
    return chr0n;
  }

  static public function ensure_dir(path:String):Void {
    if (!FileSystem.exists(path)) {
      FileSystem.createDirectory(path);
    }
  }
}
