// The solar clock: the `system` mode resolves light/dark from the SUN —
// hikari's canonical rule (`theme/useSolarTime`: solar altitude above
// +6° is day, civil twilight and night are dark), first-painted from
// the timezone-offset longitude estimate and re-checked every five
// minutes so dawn and dusk flip a live face. The installer deliberately
// runs the ESTIMATE only — it never phones a geolocation service
// (hikari's full provider cascade stays a webui concern), and the
// timezone longitude already tracks solar time to within DST-sized
// edges. The egui faces run the same arithmetic in shell/src/solar.rs.
import {
  getTimePeriod,
  timezoneFallback,
} from "@celestia-island/hikari/theme/useSolarTime";

/** How often the day/night verdict is re-checked (hikari's cadence). */
export const SOLAR_TICK_MS = 5 * 60 * 1000;

/** The sun's verdict right now: true when the face should run dark. */
export function solarWantsDark(): boolean {
  const { lat, lng } = timezoneFallback();
  return getTimePeriod(lat, lng) !== "day";
}

/**
 * Re-check the sun every `SOLAR_TICK_MS`; `onFlip` fires only when the
 * verdict changes. Returns the stopper (a Vue `onBeforeUnmount` target).
 */
export function startSolarClock(onFlip: (dark: boolean) => void): () => void {
  let dark = solarWantsDark();
  const timer = setInterval(() => {
    const next = solarWantsDark();
    if (next !== dark) {
      dark = next;
      onFlip(dark);
    }
  }, SOLAR_TICK_MS);
  return () => clearInterval(timer);
}
