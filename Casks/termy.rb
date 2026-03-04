cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.39"
  sha256 arm:   "bd208489d3b54fec0e12f781f4fde39ded65e8bafa9d7da951f5ad839356ebd6",
         intel: "001423c1d7372dd227537c681770185ce83fb435a962c6a6cc701547be97659c"

  url "https://github.com/lassejlv/termy/releases/download/v#{version}/Termy-v#{version}-macos-#{arch}.dmg"
  name "Termy"
  desc "Minimal GPU-powered terminal written in Rust"
  homepage "https://github.com/lassejlv/termy"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :big_sur"

  app "Termy.app"
end
