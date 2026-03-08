cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.47"
  sha256 arm:   "44baf06968d0cd044173efcc18f61b2e7a356e96657e2e6e4b3e74be436740e4",
         intel: "92a9f6e27fb25740d7e8a7a6b6735bf0958b0fec251eda27dcc40531de4f05c0"

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
