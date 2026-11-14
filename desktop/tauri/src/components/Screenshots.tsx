import { useState } from "react";
import { ImageOff } from "lucide-react";
import { api } from "@/lib/api";
import { Lightbox } from "@/components/Lightbox";
import { useT } from "@/lib/i18n";

interface ScreenshotsProps {
  images: string[];
}

// Thumbnail grid of mod screenshots. Images load through the authenticated
// backend proxy (DeadlyStream hotlink-protects the raw URLs). Clicking a
// thumbnail enlarges it in an in-app lightbox (never opens a browser tab).
export function Screenshots({ images }: ScreenshotsProps) {
  const t = useT();
  const [lightbox, setLightbox] = useState<string | null>(null);
  const [broken, setBroken] = useState<Record<string, boolean>>({});

  if (images.length === 0) return null;

  return (
    <section className="space-y-2">
      <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        {t("modDetail.screenshots")}
      </h3>
      <div className="flex flex-wrap gap-2">
        {images.slice(0, 12).map((raw, i) => {
          const proxied = api.imageProxy(raw);
          const isBroken = broken[raw];
          return (
            <button
              key={`${raw}-${i}`}
              type="button"
              onClick={() => !isBroken && setLightbox(proxied)}
              title={t("modDetail.openScreenshot")}
              className="group relative flex size-24 items-center justify-center overflow-hidden rounded-md border bg-muted"
            >
              {isBroken ? (
                <ImageOff className="size-5 text-muted-foreground" />
              ) : (
                <img
                  src={proxied}
                  alt=""
                  loading="lazy"
                  className="size-full object-cover transition-transform group-hover:scale-105"
                  onError={() => setBroken((b) => ({ ...b, [raw]: true }))}
                />
              )}
            </button>
          );
        })}
      </div>
      {lightbox && <Lightbox src={lightbox} onClose={() => setLightbox(null)} />}
    </section>
  );
}
