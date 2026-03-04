cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.37"
  sha256 arm:   "53174a8511b8becac9b475032d17b4e540103798dd6cd00cbf85b0a1f66ebd22",
         intel: "03aefb7aa1ebea9f18e1ec176f70e95c0b1e8e228c114f7c18ef65aa6cba00f6"

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
