export type NotificationSeverity = "info" | "warn" | "error";

export interface InAppNotification {
  title: string;
  message: string;
  severity: NotificationSeverity;
}

export type InAppNotificationFallback = (notification: InAppNotification) => void;

export interface NotificationService {
  notify(
    title: string,
    body: string,
    level: string,
    fallback: InAppNotificationFallback,
  ): Promise<void>;
}

export class BrowserNotificationService implements NotificationService {
  private permission: NotificationPermission | null = null;

  async notify(
    title: string,
    body: string,
    level: string,
    fallback: InAppNotificationFallback,
  ): Promise<void> {
    const severity = normalizeSeverity(level);
    const fallbackToast = () => fallback({ title, message: body, severity });

    if (!canUsePlatformNotifications()) {
      fallbackToast();
      return;
    }

    let permission: NotificationPermission;
    try {
      permission = await this.ensurePermission();
    } catch {
      fallbackToast();
      return;
    }
    if (permission !== "granted") {
      fallbackToast();
      return;
    }

    try {
      new Notification(title || "Neoism", { body });
    } catch {
      fallbackToast();
    }
  }

  private async ensurePermission(): Promise<NotificationPermission> {
    if (this.permission !== null) {
      return this.permission;
    }
    const current = Notification.permission;
    if (current === "granted" || current === "denied") {
      this.permission = current;
      return current;
    }
    try {
      this.permission = await Notification.requestPermission();
    } catch {
      this.permission = "denied";
    }
    return this.permission;
  }
}

function normalizeSeverity(level: string): NotificationSeverity {
  return level === "warn" || level === "error" ? level : "info";
}

function canUsePlatformNotifications(): boolean {
  if (typeof Notification === "undefined") {
    return false;
  }
  if (typeof window !== "undefined" && window.isSecureContext === false) {
    return false;
  }
  return true;
}
