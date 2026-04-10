"use client";

import { useEffect, useRef } from "react";

const SCROLL_DEPTH_PX = 3000;

export function OceanBackground() {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    let ticking = false;
    const onScroll = () => {
      if (ticking) return;
      ticking = true;
      requestAnimationFrame(() => {
        if (video.duration) {
          const progress = Math.min(window.scrollY / SCROLL_DEPTH_PX, 1);
          video.currentTime = progress * video.duration;
        }
        ticking = false;
      });
    };

    const onLoaded = () => onScroll();
    video.addEventListener("loadedmetadata", onLoaded);
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      video.removeEventListener("loadedmetadata", onLoaded);
      window.removeEventListener("scroll", onScroll);
    };
  }, []);

  return (
    <>
      <video
        ref={videoRef}
        src="/ocean-bg.mp4"
        muted
        playsInline
        preload="auto"
        className="fixed inset-0 w-full h-full object-cover -z-10 pointer-events-none"
      />
      <div className="fixed inset-0 bg-black/40 -z-10 pointer-events-none" />
    </>
  );
}
