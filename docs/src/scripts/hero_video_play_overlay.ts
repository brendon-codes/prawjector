const video_events = ["play", "playing", "pause", "ended"] as const

const is_video_active = (video: HTMLVideoElement) =>
  !video.paused && !video.ended

const sync_overlay = (video: HTMLVideoElement, overlay: HTMLButtonElement) => {
  overlay.hidden = is_video_active(video)
}

const play_video = async (
  video: HTMLVideoElement,
  overlay: HTMLButtonElement
) => {
  try {
    await video.play()
  } catch {
    sync_overlay(video, overlay)
  }
}

const bind_overlay = (overlay: HTMLButtonElement) => {
  const video = globalThis.document.querySelector<HTMLVideoElement>(
    `#${overlay.dataset.videoId ?? ""}`
  )

  if (video === null) {
    return
  }

  for (const event_name of video_events) {
    video.addEventListener(event_name, () => sync_overlay(video, overlay))
  }

  overlay.addEventListener("click", () => play_video(video, overlay))
  sync_overlay(video, overlay)
}

for (const overlay of globalThis.document.querySelectorAll<HTMLButtonElement>(
  "[data-video-play-overlay]"
)) {
  bind_overlay(overlay)
}
