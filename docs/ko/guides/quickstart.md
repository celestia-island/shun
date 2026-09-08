# 빠른 시작

shun은 두 부분으로 구성됩니다: **빌드 측**(payload 패키징, 배포 매니페스트 해결)과
**런타임 측**(플로우를 구동하는 셸).

## 데모 체험

```bash
cargo run --example demo_flash                        # 쓰기 후보 장치 열거
cargo run --example demo_install                      # ShunDemo.shun 생성 + 로컬 설치
cargo run --example demo_install -- --portable        # 휴대용 설치 (레지스트리 미사용)
cargo run --example demo_install -- --uninstall       # 제거 (모든 흔적 제거)
```

`demo_install`은 설치 패키지 `ShunDemo.shun`을 생성하고, 스트리밍 진행률과 함께
압축을 해제하며, 로컬 모드에서는 직접 Windows 등록을 수행합니다: 사용자 단위 ARP
항목(설정 → 앱), 시작 메뉴 바로 가기, 자가 복사 제거 프로그램. 휴대용 모드는
`.shun-portable` 마커만 쓰고 레지스트리는 건드리지 않습니다.

## 데모 셸 실행

```bash
pnpm --dir shell/web install
cargo run -p shun_demo_shell
```

셸은 빌드 시점에 데모 payload를 내장(단일 파일 설치 패턴)하며,
`shell/Cargo.toml` → `[package.metadata.shun]`에 선언된 배포 모드를 렌더링합니다.
