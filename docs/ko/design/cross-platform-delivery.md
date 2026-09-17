# 설계 노트: 크로스 플랫폼 배포

상태: **Windows 표면은 검증을 마쳤고 갭은 메워졌습니다(2026-09);
Linux와 macOS의 런타임 등록 백엔드는 구현되었습니다; 모바일
(Android/iOS)과 HarmonyOS는 조사 완료 — 당장은 범위 밖입니다.** 이
노트는 (a) 자동 패키징 등록 표면에 대한 집중 테스트가 출하된 Windows
백엔드에서 입증한 바, (b) 과거에 결여되었던 바탕 화면 바로 가기 /
작업 표시줄 식별자 / 상황에 맞는 메뉴 표면 — 현재는 구현되었으며 그것을
빚은 실기 보안 정책 발견과 함께, (c) Linux와 macOS 백엔드, (d)
Android, iOS, HarmonyOS에 대한 실현 가능성 판정(보류)을 기록합니다.

(a)의 실행형 인벤토리는 `tests/registration_shortcuts.rs`와
`tests/msix_pack.rs`에 있습니다.

## 1. 집중 Windows 테스트가 입증한 것 (2026-09, 실기)

Windows 11 실기에서 실제 `InstallFlow`를 실행하고, 그 결과를 Windows
스스로를 통해 다시 읽는 방식으로 검증했습니다(WScript.Shell COM
리졸버, 레지스트리, Windows SDK의 MakeAppx) — 우리 자신의 출력을
들여다보는 방식이 아니라:

| 표면 | 결과 |
| --- | --- |
| 시작 메뉴 `.lnk`가 shell을 통해 해석됨 | **통과** — `TargetPath`와 `WorkingDirectory`가 설치 컨텍스트가 선언한 그대로 돌아옵니다; `mslnk`의 손수 만든 합성 PIDL이 올바르게 해석됩니다 |
| 비 ASCII 설치 경로(중국어 디렉터리) | **통과** — `顺测试目录` 설치 디렉터리가 COM 리졸버를 거쳐 바이트까지 정확하게 왕복합니다 |
| `.lnk` 바이너리 대 MS-SHLLINK | **통과** — 헤더, CLSID `{00021401-…}`, 플래그 집합(target ID list + relative path + working dir + unicode), 핫키 없음 |
| ARP 항목 (HKCU) | **통과** — 완전한 NSIS 동등 필드 집합: DisplayName/Version/Publisher/InstallLocation/DisplayIcon에 더해 `UninstallString`/`ModifyPath`/`RepairString`(모두 인용 처리, 각각 `/uninstall`을 통해 제거 프로그램 UI 호출)과 DWORD `EstimatedSize`; EstimatedSize는 payload 매니페스트 총량과 일치 |
| 제거 정리 | **통과** — ARP 키, 바로 가기, payload, 제거 프로그램, 디렉터리 모두 제거(`tests/install_local.rs`가 커버, 여기서 재확인) |
| MSIX 매니페스트 생성 | **통과** — 아이덴티티, 네 파트 패딩 버전, XML 이스케이프된 문자열, 정슬래시 진입점, runFullTrust |
| MSIX 실제 패킹 (MakeAppx 10.0.26100) | **통과** — `[Content_Types].xml` + `AppxManifest.xml`을 갖춘 유효한 OPC zip; 산출물 `dist/shundemo-0.1.0-x64.msix`에는 **서명 블록이 없습니다**(의도된 설계: 스토어 배포 시 대신 서명되거나, 자체 서명 인증서를 신뢰해야 함) |

## 2. Windows 갭 보완 작업 (2026-09 구현)

아래의 모든 것은 검증 패스 이후에 반영되었으며, 각 표면은 실기에서
등록 테스트 스위트로 구동되었습니다.

### 바탕 화면 바로 가기 — `install.desktop-shortcut`

정책(`always` | `never` | `ask`, NSIS 체크박스 관례 — egui 마법사는
`ask`에 대해 기본 선택된 토글을 보여주고, 헤드리스 실행은 선택됨으로
응답합니다)에서 해석되어 시작 메뉴 바로 가기 옆에 기록됩니다. 바탕
화면은 **`SHGetKnownFolderPath(FOLDERID_Desktop)`**로 해석합니다 —
바탕 화면이 리다이렉트될 때(OneDrive, 도메인 정책) 틀려지는
`%USERPROFILE%\Desktop`은 결코 쓰지 않습니다. 제거는 무조건
지웁니다(설치와 제거 사이에 구성이 바뀌었을 수 있으므로).

**실기 실측**: 보안 정책(AV/EDR의 가짜 바로 가기 및 랜섬웨어 방지)은
흔히 바탕 화면에서의 `.lnk` 생성을 *특히* 거부합니다 — 검증 머신에서는
상승된 셸의 `echo x > Desktop\probe.lnk`조차 거부된 반면 `.tmp`
파일은 자유롭게 써졌습니다. 따라서 바탕 화면 바로 가기는
**best-effort**입니다: 거부된 쓰기는 경고로 강등되며 설치를 절대
실패시키지 않습니다(시작 메뉴 바로 가기와 ARP 항목이 중요
표면입니다). 테스트 스위트는 머신의 정책을 프로브하고 정상 경로와
우아한 강등 경로 모두를 단언합니다.

### 작업 표시줄 식별자 — `install.aumid`

프로그래밍 방식 작업 표시줄 고정은 여전히 **플랫폼 설계상
차단**됩니다(지원되는 API 없음; 고정 핵은 Windows 10에서
제거되었습니다). 제공된 것은 식별자 절반입니다: 모든 바로 가기에
Shell COM 속성 저장소(`IShellLink` → `IPersistFile` →
`IPropertyStore`, `src/targets/aumid.rs`에 있음 — `mslnk`는 바이트만
씁니다)를 통해 **`System.AppUserModel.ID`**가 찍혀 있어, 작업 표시줄
그룹화, 점프 목록, 그리고 *사용자가 시작하는* 고정이 올바르게
동작합니다. 기본 AUMID는 `{publisher}.{product}`로 생성됩니다;
`install.aumid`가 재정의하며, 앱은 같은 값을
`SetCurrentProcessExplicitAppUserModelID`에 전달해야 합니다. MSIX
설치는 패키지를 통해 식별자를 공짜로 얻습니다. 스탬핑은 같은 정책
사유로 best-effort입니다(검증 머신은 `.lnk` 파일에 대한
`IPropertyStore::SetValue`를 거부합니다 — 0x80030005 — 따라서
거기서는 스탬프가 경고로 강등됩니다).

### 상황에 맞는 메뉴 동사 — `[[install.verbs]]`

티어 1 제공됨: `HKCU\Software\Classes\Applications\<exe>\shell\<verb>\command`
아래의 사용자 단위 탐색기 동사 — 문서화된 Application Registration
표면, 권한 상승 불필요, 앱의 exe와 그것을 가리키는 바로 가기에
나타납니다. 세 가지 동사 대상은 이를 구현하는 모든 플랫폼에서
명령줄로 매핑됩니다: `data-folder`(설치 디렉터리 열기),
`uninstall`(복사된 제거 프로그램 실행), `app`(진입점 + 인수). 제거는
자신이 만든 동사 키를 지운 뒤, `shell`/`Applications` 컨테이너는
비어 있을 때만 지웁니다(다른 누군가 등록한 동사는 살아남습니다).
티어 2(파일 형식 연결)와 티어 3(MSIX `FileExplorerExtension`)은
향후 과제로 남습니다.

### 딥 링크 — `install.deep-links`

배포 모델이 첫 초안부터 약속했던 것; 이제 모든 백엔드에서, 사용자
단위로 실재합니다: Windows는 각 스킴을
`HKCU\Software\Classes\<scheme>` 아래 프로토콜 클래스로 등록합니다
(빈 `URL Protocol` 마커 + URL을 `%1`로 받는 열기 명령); Linux는
런처에 `MimeType=x-scheme-handler/<scheme>;`을 선언하고 `xdg-mime`로
기본을 주장합니다; 합성된 macOS `Info.plist`는 `CFBundleURLTypes`를
휴대합니다. 스킴은 소문자 `[a-z0-9+.-]`로 정규화됩니다
(`"MyApp://"` → `myapp`). 제거는 Windows 프로토콜 클래스를
삭제합니다; Linux 런처를 제거하면 핸들러가 고아가 됩니다
(`mimeapps.list` 줄이 무력해짐 — 기록했고, 수용했습니다).

### 설치 관리자가 UAC 권한 상승을 요청하지 않는 이유

이 질문은 바탕 화면 바로 가기 발견 이후에 나왔고, 답에는 세 가지
다리가 있습니다:

1. **shun이 쓰는 모든 것은 사용자 단위 표면입니다** — HKCU, 사용자의
   시작 메뉴와 바탕 화면, `%LOCALAPPDATA%`. 어느 것도 상승된 토큰이
   필요하지 않으므로 UAC 프롬프트는 아무것도 사 주지 못하면서
   최악의 종류의 프롬프트 소음(사용자가 클릭을 무심코 지나치도록
   훈련시키는)만 더할 뿐입니다. 이것은 wowsp NSIS 템플릿이 만든
   트레이드이자, VS Code / Chrome *사용자* 설치 뒤에 있는 것과
   같습니다.
2. **권한 상승은 어차피 `.lnk` 차단을 고치지 못합니다**: 차단은 바탕
   화면 폴더와 `.lnk` 확장자에 키가 맞춰진 보안 제품의 파일 시스템
   필터입니다 — 필터는 ACL이 아니라 정책에 따라 프로세스를 가로채며,
   상승된 프로세스도 필터링됩니다. (Windows 제어 폴더 액세스도 같은
   방식으로 동작합니다: 앱이 허용 목록에 없으면 관리자도
   차단합니다.) 올바른 대응은 제공된 그것입니다: 경고로 강등하고,
   시작 메뉴 바로 가기와 ARP를 온전히 지킵니다.
3. ***사용자* 표면에 대한 상승된 쓰기는 정확성 함정입니다**: 상승된
   프로세스는 프로필을 다르게 해석합니다(관리자 계정의 바탕 화면,
   `%APPDATA%`, 레지스트리 하이브 모두 설치한 사용자의 것과 다를 수
   있음) — 고전적인 NSIS 전체 사용자 바로 가기 버그입니다. 권한 상승이
   정말로 필요할 때는 해당 단계가 *스스로* 상승합니다: WebView2
   Evergreen 부트스트래퍼는 자체 `requireAdministrator` 매니페스트를
   휴대하므로, 셸은 `asInvoker`로 남아 위임합니다.

머신 전체 범위(`Program Files`, HKLM ARP, 전체 사용자 바로 가기)는
일부 제품이 필요로 하는 정당한 *모드*입니다 — 정확히 그렇게
제공되었습니다: 신중한 옵트인(`install.scope`), 결코 기본값이
아닙니다. 6절을 보세요.

### 견고성 발견 — 둘 다 수정됨

- 파일명으로 불법인 문자(`/\:*?"<>|`, 끝의 점/공백)를 포함한 제품
  이름은 모든 파일 시스템과 레지스트리 표면(`.lnk` 이름, ARP 키
  경로)에서 **스템 정제**(stem-sanitize)됩니다 — 제품 이름 속의 `\`가
  더 이상 레지스트리 서브키를 중첩해 만들지 않습니다.
- ARP `UninstallString`은 `/uninstall`을 전달하지만, 설치 셸의
  헤드리스 파서는 `--uninstall`만 받아들였습니다 — Windows 설정에서
  "제거"를 클릭하면 제거 대신 마법사가 열렸던 것입니다. 이제 두
  표기 모두 파싱됩니다.

## 3. Linux와 macOS (런타임 백엔드 구현됨; 패키징 산출물은 다음)

**둘 다 다룰 만하며, 둘 다 Windows와 하나의 경질 한계를 공유합니다:
프로그래밍 방식 작업 표시줄/Dock 고정은 그 어디에도 존재하지
않습니다.** 런타임 `Registration` 백엔드는 제공되었습니다; 빌드 측
패키징 산출물(`tauri-bundler`를 통한 deb/rpm, DMG)은 각자의 네이티브
빌드 호스트가 필요하기에 후속 과제로 남습니다.

### Linux — `LinuxRegistration` (src/targets/freedesktop.rs)

Windows 백엔드가 하는 모든 일은 freedesktop 관례로 매핑되며, 전부
사용자 단위(`~/.local/share/...`)이고 권한 상승이 없습니다:

- **런처 등록** = `<product>.desktop`(Name, Exec, `install.icon`의
  Icon, Categories, 그리고 결정적으로 **`StartupWMClass`** = 진입
  실행 파일 스템 — 사용자가 시작한 작업 표시줄/Dock 고정이 올바른
  아이콘 아래 그룹되게 만드는 필드)을 `~/.local/share/applications`에
  쓴 뒤 그것에 대해 `update-desktop-database`를 실행합니다(도구가
  없으면 건너뛰어도 안전 — 데스크톱 환경은 지연 재스캔);
- **상황에 맞는 메뉴 동사 + 제거 항목** = `Actions=` +
  `[Desktop Action <id>]` 그룹 — 항상 **Uninstall** 액션을 포함하는데,
  GNOME Software / KDE Discover가 자기 패키지 백엔드가 추적하는 앱만
  나열하기 때문입니다: shun으로 설치된 앱은 거기 절대 나타나지
  않습니다. 세 가지 동사 대상은 `xdg-open`, 제거 프로그램, 진입점 +
  인수로 매핑됩니다;
- **실행 비트 복원** — payload 아카이브가 모든 항목에 0644를 싣고
  있으므로, 백엔드는 진입점과 복사된 제거 프로그램을 0755로 다시
  chmod합니다;
- **해지** = `.desktop` 삭제 + 데이터베이스 새로 고침.

`.desktop` 라이터는 모든 플랫폼에서 컴파일되고(단위 테스트되는)
단순한 데이터 배관입니다; 프로세스 실행 절반만 Linux 게이트입니다.
작업 표시줄/Dock **고정은 여전히 불가능합니다**(크로스 데스크톱 API
없음: GNOME 즐겨찾기는 내부 gsettings 키이고 KDE 고정은 문서화되지
않은 appletsrc에 삽니다; 사용자 동작으로 취급). 파일 관리자 상황에
맞는 메뉴(Nautilus 스크립트 / Dolphin 서비스 메뉴)는 범위 밖으로
남습니다.

**패키징 포맷**(여전히 빌드 측, 후속): **tarball/휴대용(shun이 이미
보유) + deb([cargo-deb]) + rpm([cargo-generate-rpm])**이 최선의
부분집합입니다 — 정확히 `tauri-bundler`가 내보내는 것입니다(비-Tauri
payload에도 쓸 수 있는 라이브러리이며, [cargo-packager]도 그렇습니다).
AppImage = 중간; Flatpak = 중상; snap = 높음, 보류. 정직한 "모든
배포판에서 실행" 한계: glibc는 앞으로만 호환되며 **musl 정적은 WebView
앱을 실을 수 없습니다**(webkit2gtk가 GTK C 스택을 끌고 다닙니다) —
배포 셸은 musl-정적일 수 있지만, 배포되는 Tauri 앱은 지원되는 가장
오래된 webkit2gtk-4.1 기준선(Ubuntu 22.04 / Debian 12 / Fedora 37+
시대)에 맞춰 빌드해야 합니다.

### macOS — `MacOSRegistration` (src/targets/macos.rs)

- **등록** = 진입 실행 파일이 들어 있는 `.app` 번들을 찾고(가장
  가까운 `.app` 조상); payload가 `Info.plist`를 휴대하지 않았다면
  최소한의 것을 합성합니다(`plist.rs`, 순수하고 모든 곳에서 단위
  테스트됨); 진입점의 실행 비트를 복원; 설치 전체에서 상속된
  **`com.apple.quarantine`을 재귀적으로 벗겨냅니다**(브라우저가 설치
  관리자에 격리 도장을 찍고, macOS 복사는 xattr을 보존합니다 — 이걸
  안 하면 배포된 앱이 사용자가 이미 답한 Gatekeeper 제어를 상속합니다);
  그다음 번들을 `lsregister -f`합니다 — Spotlight와 Launchpad가
  따라옵니다. 해지 = `lsregister -u`(파일은 일반 제거 패스가 지웁니다).
  벌거벗은 실행 파일 payload(`.app` 없음)는 no-op으로 등록됩니다 —
  휴대용 관례;
- **Dock 고정**: **지원되는 API 없음**(`defaults write
  com.apple.dock` + `killall Dock` 핵은 사용자 기본 설정을 짓밟고
  최근 macOS에서 불안정합니다) — LS 등록을 통한 Launchpad/Spotlight
  존재가 탐색성의 등가물입니다;
- **서명은 실제 배포를 위해 여전히 필수 프로세스 작업입니다**:
  Developer ID + hardened runtime + `notarytool` + staple; 그리고
  다운로드된 셸은 **전치(translocate)될 것입니다** — 자기 경로 가정을
  그에 맞게 다루세요;
- **패키징 산출물**(후속): tauri-bundler / `hdiutil`을 통한 DMG,
  `.pkg`는 관리자 흐름에만, Homebrew cask가 한 채널입니다.
  **universal2**로 출하(이중 빌드 + `lipo` + 재서명).

[cargo-deb]: https://github.com/kornelski/cargo-deb
[cargo-generate-rpm]: https://crates.io/crates/cargo-generate-rpm
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager

## 4. Android와 iOS (조사됨)

**프레임 재정의부터: 모바일에서 "설치"는 플랫폼 소유이며 서명
검증됩니다. 앱 디렉터리의 zstd tar를 사용자가 고른 위치로 스트리밍하는
것의 등가물은 존재하지 않습니다.** shun의 정직한 모바일 역할은
**빌드/패키징/서명 파이프라인**("모바일용 cargo-dist")이지, 설치
관리자 런타임이 아닙니다.

### Android

- **패키징 메커니즘**: APK = 서명된 zip(DEX + 리소스 + ABI별
  `.so`); 파이프라인 `aapt2` → `d8`/`r8` → 패키지 → `zipalign` →
  `apksigner`. AAB는 Play 게시 포맷입니다(신규 앱 필수); 사이드로딩에는
  구체적인 APK가 필요합니다(`bundletool build-apks` + 재서명). Rust
  타깃은 호스트 도구를 갖춘 Tier 2입니다.
- **살아 있는 도구(2025–2026)**: Tauri 2의 `tauri android
  build`(cargo-mobile2 + Gradle 포장; keystore는
  `keystore.properties`), **cargo-ndk**(유지 관리 중), **`apk`
  crate**(Gradle 없는 aapt2/d8/zipalign/apksigner). **xbuild와
  cargo-apk는 죽었거나 잠들었습니다** — 그 위에 세우지 마세요. Tauri 2
  모바일은 공식적으로 안정; Android 쪽이 iOS보다 성숙합니다.
- **런타임 설치 관리자: 실현 불가능/무의미.** OS 패키지 관리자는
  서명된 APK를 사용자 대면 확인과 함께 설치합니다(장치 소유자/MDM만
  무음); **APK가 곧 설치 관리자입니다**. Play 정책은 더해서 **자체
  갱신 / 실행 코드 다운로드를 금지**합니다 — shun식 업데이터 런타임은
  Play 배포 앱이라면 모두 정책 위반입니다. 사이드로딩은 설계상
  저하됩니다: **검증된 개발자 사이드로딩 강제가 2026-09-30부터
  시작됩니다**(미검증 앱은 24시간 대기가 붙은 여러 단계 흐름을 밟습니다).
- **바로 가기**: 홈 화면 아이콘과 앱 바로 가기(`shortcuts.xml`,
  `ShortcutManager`, `requestPinShortcut`)는 **앱 선언 전용**입니다 —
  설치 시점 주입 API는 존재하지 않습니다. 빌드 시점 등가물은 shun이
  꾸리는 APK에 `shortcuts.xml`/intent-filter를 생성하는 것입니다.

### iOS

- **패키징 메커니즘**: IPA = `Payload/App.app` +
  `embedded.mobileprovision`(App ID + 엔타이틀먼트 + 인증서 + Ad Hoc
  UDID 허용 목록)을 담은 zip. 채널: App Store, TestFlight, Ad Hoc(유형
  당 연 100대), Enterprise. **서명 도구 체인(codesign, xcodebuild,
  키체인)은 macOS 전용** — 하드 호스트 요구 사항입니다.
- **런타임 설치 관리자: 두 틈새 밖에서는 실현 불가능**: (a) Ad Hoc OTA
  매니페스트(`itms-services://?...manifest.plist`) — 쉽고 합법적이며
  니치; (b) EU Web Distribution / 대안 마켓플레이스 체제 — 실재하지만
  Apple 자격 + 공증 + 2026년 10월 요율 조건(5% Core Technology
  Commission)의 문 뒤에 있고 EU 전용입니다. 무료 Apple ID 사이드로딩
  (AltStore/Sideloadly)은 7일/3앱의 취미용 경로이지 제품화 가능한
  채널이 아닙니다.
- **바로 가기**: 설치 시점에는 어떤 채널에서도 아무것도 존재하지
  않습니다 — 홈 화면 아이콘, URL 스킴, universal link는 앱이 선언하고
  서명이 검증합니다.
- **egui 폴백**: Android = `android-activity` + winit + wgpu
  (Vulkan/GLES); iOS = winit + wgpu(Metal)가 FFI + Xcode 프로젝트를
  통해 UIKit 호스트에 내장됩니다. 둘 다 턴키 스토리가 없습니다 — shun
  패키징 파이프라인이 정확히 그 빠진 조각입니다.

## 5. HarmonyOS (조사됨)

**결론부터: "shun이 HarmonyOS를 지원한다"는 말이 2026년에 정직하게
의미할 수 있는 것은 정확히 하나입니다 — 릴리스 서명된 HAP/APP 산출물을
만들어내는 빌드 시점 패키징/서명 타깃, 더해 `hdc install` 개발 장치
흐름. 런타임 설치 관리자(shun의 NSIS 절반)는 HarmonyOS NEXT에 법적·
기술적 기반이 없습니다.**

풍경(2025–2026): HarmonyOS NEXT(5.0, 2024-10)가 APK 호환 계층을
버렸습니다; 겨냥할 라인은 HarmonyOS 6+(API 20/23), 중국 전용,
AppGallery 전용, 중국 OS 시장의 약 19%. OpenHarmony가 열린 베이스
이며; 상업 HarmonyOS는 그 위에 얹힌 화웨이의 제품입니다 — 패키저는
상업판을 겨냥합니다.

- **패키지 포맷**: HAP(zip: `module.json5`, ArkTS 바이트코드, 네이티브
  `libs/<abi>/*.so`); HSP/HAR 공유 패키지; `.app` = AppGallery 제출
  팩(`pack.info`). 도구는 CLI로 쓸 수 있습니다: `ohpm` + `hvigorw
  assembleHap` + `hap-sign-tool` + `app_packing_tool.jar`(공식 지원
  헤드리스 CI 빌드).
- **서명**: SHA256withECDSA; `.p12` 키스토어 + `.cer` + `.p7b`
  프로파일(번들 이름, 권한, debug의 경우 장치 UDID 허용 목록); 인증서는
  **AppGallery Connect를 통해 화웨이가 발급**합니다(개인 등록 무료 —
  Apple식 비용 없음).
- **Rust**: `aarch64/armv7/x86_64-unknown-linux-ohos`는 **호스트 도구를
  갖춘 Tier 2**입니다(1.78부터 rustup 준비 완료). 커뮤니티 `ohos.rs`
  도구 체인(`cargo-ohos`, `napi-ohos`, `ohos-openssl`)이 접착제입니다;
  Rust 코어 + ArkTS 셸은 검증된 아키텍처입니다(RustDesk OHOS). **egui는
  차단됨**: winit에 업스트림 OHOS 백엔드가 없습니다(커뮤니티 베타만).
  **Tauri**: 공식이지만 미병합인 `feat/open-harmony` 브랜치(wry/tao
  패치, `cargo tauri ohos` CLI)는 오늘 동작하지만 빠르게 움직입니다 —
  몇 달마다 재핀을 예상하세요.
- **분배의 정직함**: 소비자 사이드로딩은 사실상 닫혀 있습니다
  (AppGallery만; `hdc install`에는 개발자 모드 + 화웨이 서명 + UDID가
  필요). 지정 장치 릴리스: 연 100대, 90일 유효. 엔터프라이즈 배포는
  현재 칭윈(Qingyun) 엔터프라이즈 PC로 한정됩니다. **HarmonyOS PC**는
  실재합니다(ARM, 스토어 배포, 아직 사이드로딩 없음) — 화웨이는 PC
  사이드로딩을 나중에 열 *의향을 밝혔습니다*; 언젠가 그곳에서 데스크톱
  배포 런타임을 정당화할 수 있는 유일한 관찰 항목이 그것입니다.

작업량 표: HAP 패키징 타깃 **중간**; Rust 교차 단계 **쉬움–중간**(링커로
SDK clang 래퍼, TLS는 ohos-openssl); Tauri-on-OHOS 패키징 **중간–어려움**
(업스트림 미병합); egui 폴백 **어려움**(winit 없음); 런타임 설치 관리자/
플래셔 **실현 불가능**.

## 6. 설치 범위와 선언적 마법사 (2026-09 구현)

### 설치 범위 — `install.scope = user | machine | ask`

사용자 단위가 여전히 기본입니다("설치 관리자가 UAC 권한 상승을
요청하지 않는 이유" 참조). `machine` — 또는 "모든 사용자"로 답한
`ask` — 은 모든 등록 표면을 머신 전체 등가물로 뒤집습니다: **HKLM**
아래의 ARP 항목, **전체 사용자 시작 메뉴**(`%ProgramData%`)의 바로
가기, **공용 바탕 화면**(`FOLDERID_PublicDesktop`)의 바탕 화면 바로
가기, `HKLM\Software\Classes` 아래의 동사와 딥 링크. 셸은 플로우가
실행되기 **전에** 해석 결과를 감지하여, 아직 상승되지 않았다면
사용자의 답을 실은 채 `runas` 동사로 자신을 다시 실행합니다
(`--silent --mode=… --dir=… --scope=machine`): UAC 동의가 유일한
프롬프트이며, 그것이 필요한 모드에만 나타납니다 — 부트스트래퍼
패턴, 약속한 그대로입니다. 제거는 거울상입니다(ARP
`UninstallString`이 제거 프로그램을 시작하며, 그것이 같은 방식으로
상승합니다). 머신 범위는 Windows 전용입니다; Linux/macOS 백엔드는
이를 명시적으로 거부합니다. 통합 테스트는 러너가 상승된 경우에만
실행됩니다(관리자 셸의 `cargo`가 이를 구동합니다).

### 마법사 파이프라인 — `[[package.metadata.shun.steps]]`

마법사는 이제 고정된 모드 → 설치 순서 대신, 선언적이고 순서가 있으며
자유롭게 구성되는 파이프라인입니다. 다섯 가지 단계 종류:

```toml
[[package.metadata.shun.steps]]
kind = "mode"                    # 배포 모드 + 디렉터리; `ask` 토글(바탕
                                 # 화면 바로 가기, 범위)을 내장
[[package.metadata.shun.steps]]
kind = "scope"                   # 독립형 사용자/머신 선택
[[package.metadata.shun.steps]]
kind = "license"                 # 라이선스 패널 (license / license-locales)
[[package.metadata.shun.steps]]
kind = "content"                 # 사용자 지정 markdown 패널
title = "Release notes"
markdown = "notes.md"         # 매니페스트 기준 상대 경로
[[package.metadata.shun.steps]]
kind = "install"                 # 배포 실행(정확히 하나 필요)
```

`steps`가 없으면 기본 파이프라인(모드 → 선언된 경우 라이선스 →
설치)이고, 레거시 `custom-steps`는 각자의 `after` 키 뒤에 주입됩니다;
둘 다 선언하는 것은 구성 오류이며, `install` 단계가 0개 또는 여러
개인 것도 마찬가지입니다. 범위 질문과 바탕 화면 바로 가기 토글은
`ask` 정책이 패널을 만나는 곳마다 등장합니다: 모드 단계에 내장되거나,
독립형(`scope`)으로 — 개발자가 구성합니다. 콘텐츠와 라이선스 문서는
**빌드 시점에** 읽혀 `shun-steps.json`(`ShunConfig::resolve_steps`)에
인라인되므로, 런타임 설치 관리자는 파일 의존성을 휴대하지 않습니다.
egui 폴백은 전체 파이프라인을 렌더링합니다(단계별 레일, 라이선스
게이팅, 뒤로/다음 탐색); Tauri `ShellView`는 해석된 단계를 웹
프론트엔드에 노출합니다.

## 7. 포트폴리오 현황 (2026-09)

| 기능 | Win | Linux | macOS | Android | iOS | HarmonyOS |
| --- | --- | --- | --- | --- | --- | --- |
| 런타임 설치 + 등록 | ✅ 사용자 + 머신 범위 | ✅ `.desktop` 백엔드 (사용자) | ✅ `.app` 백엔드 (사용자) | 보류 | 보류 | 보류 |
| 바탕 화면/시작 메뉴 바로 가기 | ✅ 둘 다 (정책 기반) | ✅ 런처 항목 | ✅ (Launchpad/Spotlight) | 해당 없음 | 해당 없음 | 해당 없음 |
| 작업 표시줄/Dock 고정 | 식별자만 (AUMID 스탬프) | 식별자만 (StartupWMClass) | 식별자만 (LS) | 해당 없음 | 해당 없음 | 해당 없음 |
| 우클릭 메뉴 | ✅ 티어 1 동사 | ✅ 데스크톱 액션 | NSServices 추후 | 보류 | 보류 | 보류 |
| 딥 링크 | ✅ 프로토콜 클래스 | ✅ MimeType + xdg-mime | ✅ CFBundleURLTypes | 보류 | 보류 | 보류 |
| 제거 방식 | ✅ ARP + 자기 삭제 | ✅ 액션 기반 | ✅ LS 해지 + 파일 | OS | OS | OS |
| 패키징 산출물 | ✅ 설치 관리자 + MSIX | tarball ✅; deb/rpm 다음 | 번들 ✅; DMG 다음 | 보류 | 보류 | 보류 |
| 서명 현실 | Authenticode / 스토어 | 선택 사항 | 필수 (프로세스 작업) | keystore / Play | Apple 인증서 | 화웨이 AGC |

**결정에 따라 보류 (2026-09)**: Android, iOS, HarmonyOS — 4–5절의
조사는 그대로 유효합니다; 어느 것도 일정에 잡혀 있지 않습니다.

**다음 순번**:

1. `tauri-bundler`를 통한 deb/rpm 패키징 산출물(Linux CI), macOS
   호스트 레인을 통한 DMG.
2. 소비자가 필요로 할 경우 Windows 우클릭 메뉴 티어 2(파일 형식
   연결).
3. **절대 하지 않음**: 어떤 폰 OS에서도 런타임 설치 관리자/바로 가기
   주입; 그 어디서도 작업 표시줄/Dock 고정; 복잡도를 정당화할 때까지의
   snap.
