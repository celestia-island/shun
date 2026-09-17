# 설계 노트: 설치 시점 스크립팅

상태: **런너 확정 — duckscript. Python은 조사를 마쳤고 임베드 가능함을
입증; 도입은 보류.** duckscript가 유일한 스크립팅 러너입니다;
JavaScript 옵션은 폐기되었습니다(duckscript를 둘러싼 cargo-make
도구 세트가 완전하며, 그렇지 않은 부분에서는 Python 호출이
탈출구입니다). 이 노트는 그 결정과 Python 임베딩 조사를 기록합니다.

## duckscript (런너)

[`duckscriptsdk`](https://crates.io/crates/duckscriptsdk)(Apache-2.0,
cargo-make의 스크립팅 언어)는 평범한 의존성처럼 임베드됩니다: 명령
집합을 `Context`에 적재하고, shun 내장 기능을 사용자 지정 명령으로
등록하면, 스크립트가 흐름 제어와 std fs/env/net과 함께
실행됩니다. 실현 가능성 증명은 `tests/scripting_duckscript.rs`에
있습니다. justfile 자체는 임베드할 수 없습니다(`just` 크레이트는
CLI이고 안정적인 라이브러리 API가 없습니다) — duckscript가 그 가족에서
임베드 가능한 멤버입니다.

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "prepare"                 # prepare | post-install | pre-uninstall
script = "installer/prepare.dk"   # `shun build`가 패키징
```

shun 내장 기능은 duckscript 명령으로 등록됩니다: `shun_progress`,
`shun_emit`, `shun_fetch`(검증된 다운로드), 더해 SDK 자체의 std
명령(fs, env, http, process, semver, ...).

shun 래퍼에서 정규화해야 할 함정: duckscript 인자에서 Windows
백슬래시 경로는 이스케이프 문자입니다(정슬래시 경로를 전달하세요),
그리고 할당은 출력 캡처 구문입니다(`x = cmd args`).

## 임베디드 Python — 실측 (프로브 실행 2026-09)

아래의 모든 것은 실제로, 두 번 실행되었습니다: 호스트 CPython 3.13.5에서
한 번, 그리고 **동반 임베더블 런타임**에서 한 번
(`python-3.13.5-embed-amd64.zip`을 풀고 PyO3의 `pyembed_runner` 예제를
그 옆에 두어 `python313.dll`/표준 라이브러리가 동반 폴더에서
로드되게 함 — `sys.prefix`가 동반 디렉터리임을 확인했습니다):

| 기능 | 결과 |
| --- | --- |
| 실제 HTTPS (urllib + TLS) | 통과 (직접 pypi.org는 로컬에서 네트워크 차단; example.com/텐센트 미러 정상) |
| 스트리밍 SHA-256 + HMAC | 통과 |
| AES-CTR 왕복, RSA-2048 서명/검증 | 통과 — 동반 런타임에 미리 설치한 `cryptography` wheel로(`pip --target runtime/Lib/site-packages` + `python313._pth`에서 `import site` 활성화) |
| 머신 식별 정보 | MachineGuid (winreg), MAC (`uuid.getnode`), C: 볼륨 일련 번호 (ctypes `GetVolumeInformationW`) — 모두 통과 |
| TPM | ctypes가 `tbs.dll`에 올바르게 도달했습니다; 프로브 머신의 펌웨어가 TPM을 꺼두어 `Tbsi_Context_Create`가 `TBS_E_TPM_NOT_FOUND`를 반환합니다 (0x8028400F — 참고: 0x80284002가 아니며, 그것은 NULL 파라미터 구조체에서 오는 `TBS_E_BAD_PARAMETER`입니다). 호출 경로는 검증되었습니다; TPM이 켜진 하드웨어에서 같은 코드가 `TPM_PT_MANUFACTURER`를 읽습니다 |

실측 크기: 임베더블 zip **10.9 MB** / 압축 해제 **20.4 MB** /
+cryptography wheel **32.4 MB**. PyO3 러너 바이너리 자체는 약 0.2
MB입니다. 네이티브 `.pyd`를 담은 서드파티 wheel(cryptography 같은)은
변경 없이 동작합니다 — 동반 런타임 안에 넣어 배포하세요.

실제 통합을 위해 기록해 둔 함정: 임베디드 인터프리터는 drop 시
종료(finalize)되지 않습니다 — 스크립트 실행 후 stdio를 명시적으로
플러시하세요(러너 예제 참고); `eval`은 표현식만 받습니다; 동반
런타임에 pip를 쓰려면 `--target`에 더해 `._pth` 조정이 필요합니다
(또는 pip를 싣는 python-build-standalone 런타임).

## WebView2 고정 버전 임베딩 — 실측

질문: 설치 관리자가 WebView2 엔진 자체를 휴대할 수 있는가, 자기 UI와
배포된 앱 모두에 전력을 공급하면서? **기계적으로 가능 — 엔드투엔드로
증명됨**; 비용은 payload입니다.

- v151.0.4129.101 x64 고정 버전 cab: **307,241,094 바이트 ≈ 293 MB**
  압축, **661.1 MB 압축 해제**.
- 데모 셸은 압축 해제된 동반 런타임에 대해 실행되었습니다
  (`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`, 이미 `webview2_available`의
  첫 번째 프로브): UI가 렌더링되었고(오프라인 스크린샷으로 검증)
  **여섯 렌더러 프로세스 모두 시스템 Evergreen 설치가 아니라 동반
  폴더에서 왔습니다**.
- 결론: 실현 가능; 약 300 MB 비용은 **수용되었습니다**(다른 패키저들과
  비슷합니다). 이중 복사 우려는 엔지니어링으로 걷어냈습니다: 아티팩트는
  한 부만 내장합니다 — 셸이 payload의 런타임 서브트리에서 스스로
  부트스트랩합니다(`extract_prefix` 스테이징 + 압축 해제의 해시 인식
  재사용)하여 설치 관리자와 설치된 앱이 그것을 공유합니다;
  configuration.md의 WebView2 전략 절을 보세요. egui 폴백은 아무것도
  없는 머신을 위한 비용 제로 바닥으로 남습니다.

## 임베디드 Python (조사됨, 실현 가능)

**결론: 가능 — Rust는 작은 CPython을 깔끔하게 임베드할 수 있습니다.**
증명은 옵트인 `python-probe` 기능 뒤의 `tests/scripting_python.rs`입니다:
`auto-initialize`를 켠 [PyO3](https://pyo3.rs)가 인터프리터를
프로세스에 임베드하고, 표준 라이브러리로 실제 Python을 평가하며,
사용자 지정 Rust 함수를 호출하고, Python 예외를 Rust 오류로
매핑합니다. 이 기능은 기본 빌드에 절대 들어가지 않습니다; CI에서는
Windows 레그(사전 설치된 CPython에 대해 `--all-features`를 해석)만
이를 구동합니다.

자가 완결형 설치 관리자의 배포 옵션, WebView2 전략 표를 거울처럼:

| 옵션 | 휴대물 | 비고 |
| --- | --- | --- |
| `system` (기본) | 없음 | duckscript `process` 명령이 설치된 python을 호출할 수 있음; 없으면 우아하게 강등 |
| `embeddable` | Windows 임베더블 패키지 (약 12–16 MB) | 공식 `python-3.x.x-embed-amd64.zip`: `python3xx.dll` + 표준 라이브러리 zip + `._pth`, 관리자 불필요, 레지스트리 미사용 — 고정 버전 WebView2와 똑같은 사유의 사적 런타임 |
| `standalone` | python-build-standalone (약 30–60 MB) | [Astral이 관리하는](https://astral.sh/blog/python-build-standalone) 배포판(`uv`가 싣는 것); 크로스 플랫폼, 버전 고정, 완전 기능; pip/네이티브 의존성이 필요하지 않다면 과잉 |

스케치:

```toml
[package.metadata.shun.script.python]    # 선택적인 무거운 탈출구
type = "embeddable"                      # system | embeddable | standalone
```

기각/보류된 대안:

- **RustPython** (MIT, 순수 Rust) — 스스로 프로덕션 준비가 안 됐다고
  밝히며, 표준 라이브러리에 공백이 있고 C 확장 모듈이 없습니다; 언젠가는
  매력적이지만 오늘의 설치 관리자에는 아닙니다.
- **PyOxidizer / `pyembed`** — 더 높은 수준의 임베딩이지만, 프로젝트가
  유지 보수 모드입니다; 여기서는 PyO3 단독으로 충분합니다.

연결하기 전의 미해결 질문:

- 크기 예산: 옵트인하는 제품에게 아티팩트 +12–16 MB가 수용 가능한가?
  (임베딩은 매니페스트별 옵트인이므로 기본 아티팩트는 작게 유지됩니다.)
- 버전 결합: PyO3는 빌드 호스트의 CPython에 링크됩니다; 배포하는
  런타임이 일치해야 합니다. 우리가 배포할 바로 그 배포판을 겨냥해
  빌드로 고정하세요(`PYO3_PYTHON` → 푼 embeddable/standalone
  디렉터리).
- 격리: 프로세스 내 임베디드 인터프리터(단일 파일 설치 관리자 UX) vs
  서브프로세스(더 단순한 크래시 격리) — 또는 훅마다 둘 중 선택.
- 어떤 훅이 Python으로 승격될 수 있는가 (prepare만, 아니면 설치 후
  복구 경로도?).
