Name:           apic-cli
Version:        0.3.2
Release:        1%{?dist}
Summary:        Git-friendly API contracts, CLI/TUI (prebuilt)
License:        MIT
URL:            https://github.com/rizukirr/apic
ExclusiveArch:  x86_64 aarch64

Provides:       apic = %{version}-%{release}

# Prebuilt binaries: no compilation, so no -debuginfo subpackage.
%global debug_package %{nil}

%global _rel https://github.com/rizukirr/apic/releases/download/v%{version}
Source0:        %{_rel}/apic-v%{version}-x86_64-unknown-linux-gnu.tar.gz
Source1:        %{_rel}/apic-gui-v%{version}-x86_64-unknown-linux-gnu.tar.gz
Source2:        %{_rel}/apic-v%{version}-aarch64-unknown-linux-gnu.tar.gz
Source3:        %{_rel}/apic-gui-v%{version}-aarch64-unknown-linux-gnu.tar.gz
Source4:        https://raw.githubusercontent.com/rizukirr/apic/v%{version}/LICENSE

%description
apic stores API contracts as plain JSON in your repository, diffable and
reviewable like code. This package provides the apic command-line / terminal UI.

%package -n apic-gui
Summary:        Git-friendly API contracts, desktop GUI (prebuilt)
Requires:       hicolor-icon-theme
Requires:       fontconfig
Requires:       freetype
Requires:       libxkbcommon
Requires:       libwayland-client
Requires:       mesa-libGL
Recommends:     xdg-desktop-portal

%description -n apic-gui
A styled desktop GUI for apic projects, sharing the same contract engine as the
CLI. Open a project folder to browse, edit, or repair contracts.

%prep
# Nothing: the sources are flat prebuilt tarballs, extracted in %%install.

%install
rm -rf %{buildroot}
cd %{_builddir}
%ifarch x86_64
tar xf %{SOURCE0}
tar xf %{SOURCE1}
%endif
%ifarch aarch64
tar xf %{SOURCE2}
tar xf %{SOURCE3}
%endif
install -Dm0755 apic     %{buildroot}%{_bindir}/apic
install -Dm0755 apic-gui %{buildroot}%{_bindir}/apic-gui
install -Dm0644 apic-gui.desktop %{buildroot}%{_datadir}/applications/apic-gui.desktop
install -Dm0644 icon.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/apic-gui.png
cp -p %{SOURCE4} %{_builddir}/LICENSE

%files
%license LICENSE
%{_bindir}/apic

%files -n apic-gui
%license LICENSE
%{_bindir}/apic-gui
%{_datadir}/applications/apic-gui.desktop
%{_datadir}/icons/hicolor/256x256/apps/apic-gui.png

%changelog
* Sat Jun 20 2026 Rizki Rakasiwi <tech_salty_team3@salt.co.id> - 0.3.2-1
- Update to apic 0.3.2.

* Fri Jun 19 2026 Rizki Rakasiwi <tech_salty_team3@salt.co.id> - 0.3.1-1
- Initial COPR packaging: prebuilt apic-cli and apic-gui.
