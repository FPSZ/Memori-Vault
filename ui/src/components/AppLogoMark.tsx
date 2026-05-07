import appLogo from "../assets/app-logo.png";

type AppLogoMarkProps = {
  alt?: string;
  className?: string;
};

export function AppLogoMark({
  alt = "Memori-Vault logo",
  className = "h-6 w-6"
}: AppLogoMarkProps) {
  return <img src={appLogo} alt={alt} className={`object-contain ${className}`.trim()} />;
}
