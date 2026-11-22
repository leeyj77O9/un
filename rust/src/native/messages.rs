//! VM 측 native 모듈이 공유하는 에러 문자열 상수.
//!
//! NRT 크레이트는 이 모듈을 사용하지 않는다 (VM 측 전용). 문자열 값은
//! 기존 리터럴과 바이트 단위로 동일해야 하며, 변경 시 differential/exe-diff
//! 검증에서 차이가 발생한다.

/// `log` / `log2` / `log10` 등에서 0 이하의 값을 받았을 때의 메시지.
pub const EXPECTED_VALUE_GREATER_THAN_ZERO: &str = "expected value greater than 0";
/// `sqrt`에 음수를 전달했을 때의 메시지.
pub const CANNOT_COMPUTE_SQRT_NEGATIVE: &str = "cannot compute sqrt of a negative number";
/// 정규식 백트래킹 한도 초과 메시지. (`regex` 서브트리는 자체 리터럴을 유지한다.)
pub const REGEX_BACKTRACKING_LIMIT: &str = "regex backtracking limit exceeded";
