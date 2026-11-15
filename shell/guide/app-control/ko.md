# Windows가 앱 실행을 막았습니다

설치는 끝났고 파일도 그대로 있습니다. 막힌 건 실행뿐입니다. 프로그램이 **os error 4551**을 보고했는데, Windows가 **0x11C7**이라고 부르는 것과 같습니다.

![스위치 위치](guide/app-control-1.png)

## 스위치 끄기

- **①** **설정 → 개인 정보 및 보안 → Windows 보안**을 열고 **Windows 보안 열기**를 누릅니다.
- **②** **앱 및 브라우저 컨트롤**로 들어가 **스마트 앱 컨트롤**을 엽니다.
- **③** **켜기**로 되어 있으면 **스마트 앱 컨트롤 → 설정**에서 **끄기**로 바꿉니다.

![세 가지 상태](guide/app-control-2.png)

이 스위치는 되돌리기 번거롭습니다. 예전에는 다시 켜려면 Windows를 초기화해야 했지만, 최근 업데이트에서는 앱 안에서 바로 다시 켤 수 있습니다.

회색으로 막혀 있거나 항목 자체가 없다면 관리자나 다른 정책이 잡고 있는 것입니다. **이벤트 뷰어 → 응용 프로그램 및 서비스 로그 → Microsoft → Windows → CodeIntegrity → Operational**에서 **3077** 이벤트를 보세요.

![세 갈래 길](guide/app-control-3.png)

[Microsoft 스마트 앱 컨트롤 FAQ](https://support.microsoft.com/ko-kr/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

---

그래도 안 되면 **0x11C7**, 스위치 상태, 이벤트 ID를 개발자에게 보내 주세요.
