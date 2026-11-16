import { useEffect } from "react";
import { X } from "lucide-react";
import { useT } from "@/lib/i18n";

interface LightboxProps {
  src: string;
  alt?: string;
  onClose: () => void;
}

// Full-screen image viewer (no dependencies). Click backdrop or press Esc to close.
export function Lightbox({ src, alt = "", onClose }: LightboxProps) {
  const t = useT();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/85 p-6 animate-fade-in"
      onClick={onClose}
    >
      <button
        onClick={onClose}
        title={t("common.close")}
        className="absolute right-4 top-4 rounded-md p-1.5 text-white/70 transition-colors hover:bg-white/10 hover:text-white"
      >
        <X className="size-5" />
      </button>
      <img
        src={src}
        alt={alt}
        onClick={(e) => e.stopPropagation()}
        className="max-h-full max-w-full rounded-md object-contain shadow-2xl"
      />
    </div>
  );
}
