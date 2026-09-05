// Notifications Windows (toasts) : conteneur tombé, moteur en échec, disque presque plein.
// Discrètes : une même notification n'est pas répétée dans les 10 secondes.
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";

const recent = new Map<string, number>();

export async function notify(title: string, body: string, key = title + body) {
  const now = Date.now();
  const last = recent.get(key);
  if (last && now - last < 10_000) return;
  recent.set(key, now);
  try {
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (granted) sendNotification({ title, body });
  } catch {
    /* notifications indisponibles : silencieux */
  }
}
