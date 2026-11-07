// 환경을 한 줄로 보고한다. scripts/bench.ps1이 ENV 줄을 파싱한다.
import os from "node:os";

const cpus = os.cpus();
console.log(`ENV|node|os=${os.version()}|plat=${process.platform}|arch=${process.arch}|cpu=${cpus.length}|64=${process.arch === "x64"}|model=${cpus[0].model}`);
