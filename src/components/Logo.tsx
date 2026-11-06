import goat from "../../assets/logo.png";

/** Open Herd's mark: the goat of the app icon (`assets/logo.png`, the same
 *  image `tauri icon` turns into the Windows and Linux icons). */
export default function Logo({ size = 34 }: { size?: number }) {
  return (
    <img
      src={goat}
      width={size}
      height={size}
      alt=""
      aria-hidden="true"
      draggable={false}
      style={{ flexShrink: 0, objectFit: "contain" }}
    />
  );
}
