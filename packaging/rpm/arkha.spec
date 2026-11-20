# arkha.spec: Arkha's RPM, for Fedora (45 tested). Built by build.sh in a
# copy of the sources (rpmbuild --build-in-place, no source tarball), the
# crates in vendor/ when build-on.sh made it; arkha_version comes from
# there too. The library dependencies are found by rpmbuild.

# Cargo strips the program (see Cargo.toml): nothing to split off.
%global debug_package %{nil}
# No %changelog to take the files' dates from (the git history is it).
%global source_date_epoch_from_changelog 0

Name:           arkha
Version:        %{arkha_version}
Release:        1%{?dist}
Summary:        Fast graphical file manager
License:        GPL-3.0-or-later

BuildRequires:  cargo >= 1.85
BuildRequires:  rust >= 1.85
BuildRequires:  gcc
BuildRequires:  gtk3-devel
BuildRequires:  gettext

# Trash, network places, metadata (notes, emblems).
Recommends:     gvfs
# Thumbnailers run in a sandbox (without it, as they are).
Recommends:     bubblewrap
Suggests:       gvfs-smb
Suggests:       gvfs-mtp
Suggests:       gvfs-gphoto2
Suggests:       ffmpegthumbnailer
Suggests:       mediainfo
Suggests:       file-roller

%description
Arkha is a file manager for GTK desktops: icon, list, compact and columns
views, tabs and split view, an info panel, bulk renaming, checksums,
checked copies, custom actions and thumbnailers, and undo for file
operations. It uses the desktop's GTK theme, icons and bookmarks, and
shares notes, emblems and custom icons with Caja.

%build
cargo build --release --locked %{?arkha_offline}

%install
install -Dm755 target/release/arkha %{buildroot}%{_bindir}/arkha
install -Dm644 arkha.desktop %{buildroot}%{_datadir}/applications/arkha.desktop
install -Dm644 data/arkha.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/arkha.svg
install -Dm644 data/io.github.dekart811.Arkha.metainfo.xml %{buildroot}%{_metainfodir}/io.github.dekart811.Arkha.metainfo.xml
# The translations build.rs made from po/*.po.
for mo in target/release/locale/*/LC_MESSAGES/arkha.mo; do
    [ -e "$mo" ] && install -Dm644 "$mo" "%{buildroot}%{_datadir}/locale/${mo#target/release/locale/}"
done
# The manual's pages build.rs made from docs/manual.
# Never a package without them (the loop below would pass without any).
test -f target/release/manual/en/index.html
find target/release/manual -type f | while read -r f; do
    install -Dm644 "$f" "%{buildroot}%{_datadir}/arkha/${f#target/release/}"
done
%find_lang %{name}

%files -f %{name}.lang
%license LICENSE
%doc README.md
%{_bindir}/arkha
%{_datadir}/applications/arkha.desktop
%{_datadir}/icons/hicolor/scalable/apps/arkha.svg
%{_metainfodir}/io.github.dekart811.Arkha.metainfo.xml
%{_datadir}/arkha/
