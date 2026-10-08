import { createContext, useCallback, useContext, useRef, useState, ReactNode } from "react";
import { CheckCircle, XCircle, Info, X } from "lucide-react";
import styles from "./Toast.module.css";

export type ToastType = "success" | "error" | "info";

interface Toast { id: number; message: string; type: ToastType; }
interface Ctx    { toast: (message: string, type?: ToastType) => void; }

const ToastContext = createContext<Ctx>({ toast: () => {} });

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const counter = useRef(0);

  const toast = useCallback((message: string, type: ToastType = "success") => {
    const id = ++counter.current;
    setToasts((prev) => [...prev, { id, message, type }]);
    setTimeout(() => setToasts((prev) => prev.filter((t) => t.id !== id)), 3500);
  }, []);

  function dismiss(id: number) {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }

  return (
    <ToastContext.Provider value={{ toast }}>
      {children}
      <div className={styles.container} aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={`${styles.toast} ${styles[t.type]}`}>
            <span className={styles.icon}>
              {t.type === "success" && <CheckCircle size={15} />}
              {t.type === "error"   && <XCircle     size={15} />}
              {t.type === "info"    && <Info         size={15} />}
            </span>
            <span className={styles.msg}>{t.message}</span>
            <button className={styles.close} onClick={() => dismiss(t.id)}>
              <X size={13} />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export const useToast = () => useContext(ToastContext);
