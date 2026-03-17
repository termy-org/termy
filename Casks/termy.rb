cask "termy" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.55"
  sha256 arm:   "7091712935a703e85e199c4aaa35dff0dbe2ad82c2d500e0b16e284f2b5ef738",
         intel: "7ded0018d193dd33f13a64df45f6b10eed99bf716cf31839c3148e471e4532b7"

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
