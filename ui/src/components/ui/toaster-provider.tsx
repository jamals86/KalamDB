import {
  createContext,
  useCallback,
  useContext,
  useState,
  type ReactNode,
} from "react";
import {
  Toast,
  ToastClose,
  ToastDescription,
  ToastProvider,
  ToastTitle,
  ToastViewport,
} from "@/components/ui/toast";

export type ToastVariant = "default" | "success" | "destructive";

export interface ToastInput {
  title: string;
  description?: string;
  variant?: ToastVariant;
  duration?: number;
}

interface InternalToast extends ToastInput {
  id: number;
  open: boolean;
}

interface ToastContextValue {
  notify: (input: ToastInput) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

let nextId = 1;
const DEFAULT_DURATION_MS = 4000;
const ERROR_DURATION_MS = 8000;
const CLOSE_ANIMATION_MS = 400;

export function Toaster({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<InternalToast[]>([]);

  const notify = useCallback((input: ToastInput) => {
    const id = nextId++;
    setToasts((prev) => [...prev, { ...input, id, open: true }]);
  }, []);

  const handleOpenChange = (id: number, open: boolean) => {
    if (!open) {
      setToasts((prev) => prev.map((t) => (t.id === id ? { ...t, open: false } : t)));
      setTimeout(() => setToasts((prev) => prev.filter((t) => t.id !== id)), CLOSE_ANIMATION_MS);
    }
  };

  return (
    <ToastContext.Provider value={{ notify }}>
      <ToastProvider swipeDirection="right">
        {children}
        {toasts.map((t) => {
          const duration =
            t.duration ?? (t.variant === "destructive" ? ERROR_DURATION_MS : DEFAULT_DURATION_MS);
          return (
            <Toast
              key={t.id}
              open={t.open}
              variant={t.variant ?? "default"}
              duration={duration}
              onOpenChange={(open) => handleOpenChange(t.id, open)}
            >
              <div className="grid gap-1">
                <ToastTitle>{t.title}</ToastTitle>
                {t.description && <ToastDescription>{t.description}</ToastDescription>}
              </div>
              <ToastClose />
            </Toast>
          );
        })}
        <ToastViewport />
      </ToastProvider>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast must be used within <Toaster>");
  }
  return ctx;
}
