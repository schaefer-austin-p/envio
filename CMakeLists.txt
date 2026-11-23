cmake_minimum_required(VERSION 3.24)
project(envio LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

option(ENVIO_WERROR "Treat compiler warnings as errors (on in CI)" OFF)
option(ENVIO_QMLTEST "Build envio-qmltest, the QML test runner that links the Envio module" OFF)

find_package(Qt5 5.15 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 DBus)

include(FetchContent)
FetchContent_Declare(CxxQt GIT_REPOSITORY https://github.com/kdab/cxx-qt-cmake.git GIT_TAG 0.10.0)
FetchContent_MakeAvailable(CxxQt)

cxx_qt_import_crate(MANIFEST_PATH qt/Cargo.toml CRATES envio_qt LOCKED
  QT_MODULES Qt5::Core Qt5::Gui Qt5::Qml)
cxx_qt_import_qml_module(envio_qml URI "Envio" SOURCE_CRATE envio_qt)

# The push helper (crates/envio-push-helper): the push service runs it from
# push/ (the helper json's `exec` is relative to the json). Its own crate in
# the root workspace, so it builds apart from the app.
corrosion_import_crate(MANIFEST_PATH crates/envio-push-helper/Cargo.toml
  CRATES envio-push-helper LOCKED)

# Postal over QtDBus (qt/postal.cpp), called from the Rust glue. An object
# library: its objects go into each link, whatever the order of the Rust
# archive that calls them (the app and the QML test runner both link it).
add_library(envio_postal OBJECT qt/postal.cpp)
target_link_libraries(envio_postal PUBLIC Qt5::Core Qt5::DBus)
target_compile_definitions(envio_postal PRIVATE QT_NO_CAST_FROM_ASCII QT_NO_CAST_TO_ASCII QT_NO_KEYWORDS)
target_compile_options(envio_postal PRIVATE -Wall -Wextra -Wpedantic -Wshadow -Wconversion
  -Wold-style-cast -Wnon-virtual-dtor -Woverloaded-virtual -Wformat=2)
if(ENVIO_WERROR)
  target_compile_options(envio_postal PRIVATE -Werror)
endif()

# org.freedesktop.Notifications over QtDBus (qt/notify.cpp), for desktops.
add_library(envio_notify OBJECT qt/notify.cpp)
target_link_libraries(envio_notify PUBLIC Qt5::Core Qt5::Gui Qt5::DBus)
target_compile_definitions(envio_notify PRIVATE QT_NO_CAST_FROM_ASCII QT_NO_CAST_TO_ASCII QT_NO_KEYWORDS)
target_compile_options(envio_notify PRIVATE -Wall -Wextra -Wpedantic -Wshadow -Wconversion
  -Wold-style-cast -Wnon-virtual-dtor -Woverloaded-virtual -Wformat=2)
if(ENVIO_WERROR)
  target_compile_options(envio_notify PRIVATE -Werror)
endif()

add_executable(envio qt/main.cpp qml/qml.qrc qml/bindle/bindle.qrc)
target_link_libraries(envio PRIVATE envio_qml envio_postal envio_notify Qt5::Core Qt5::Gui Qt5::Qml Qt5::Quick Qt5::QuickControls2)
target_compile_options(envio PRIVATE -Wall -Wextra -Wpedantic -Wshadow -Wconversion
  -Wold-style-cast -Wnon-virtual-dtor -Woverloaded-virtual -Wformat=2)
target_compile_definitions(envio PRIVATE QT_NO_CAST_FROM_ASCII QT_NO_CAST_TO_ASCII
  QT_NO_KEYWORDS ENVIO_APP_ID="envio.aps")
if(ENVIO_WERROR)
  target_compile_options(envio PRIVATE -Werror)
endif()

# tests/qml/run.sh runs the QML tests with this: qmltestrunner cannot load
# the Rust `Envio` module, which is a static plugin.
if(ENVIO_QMLTEST)
  find_package(Qt5 5.15 REQUIRED COMPONENTS QuickTest Network)
  add_executable(envio-qmltest tests/qml/main.cpp)
  target_link_libraries(envio-qmltest PRIVATE envio_qml envio_postal envio_notify Qt5::Core Qt5::Gui Qt5::Qml Qt5::Quick
    Qt5::QuickTest Qt5::Network)
endif()

# tests/notify/run.sh: the notification glue against a fake server on a
# private session bus (#442). Needs no Rust, only the C++ glue.
option(ENVIO_NOTIFYTEST "Build envio-notifytest, the D-Bus test of qt/notify.cpp" OFF)
if(ENVIO_NOTIFYTEST)
  add_executable(envio-notifytest tests/notify/main.cpp)
  target_link_libraries(envio-notifytest PRIVATE envio_notify Qt5::Core Qt5::Gui Qt5::DBus)
endif()

# po/*.po -> share/locale/<lang>/LC_MESSAGES/envio.aps.mo
find_program(MSGFMT msgfmt REQUIRED)
file(GLOB ENVIO_PO CONFIGURE_DEPENDS ${CMAKE_SOURCE_DIR}/po/*.po)
set(ENVIO_LOCALE_DIR ${CMAKE_BINARY_DIR}/share/locale)
file(MAKE_DIRECTORY ${ENVIO_LOCALE_DIR})
foreach(po IN LISTS ENVIO_PO)
  get_filename_component(lang ${po} NAME_WE)
  set(mo ${ENVIO_LOCALE_DIR}/${lang}/LC_MESSAGES/envio.aps.mo)
  add_custom_command(OUTPUT ${mo}
    COMMAND ${CMAKE_COMMAND} -E make_directory ${ENVIO_LOCALE_DIR}/${lang}/LC_MESSAGES
    COMMAND ${MSGFMT} --check -o ${mo} ${po}
    DEPENDS ${po} VERBATIM)
  list(APPEND ENVIO_MO ${mo})
endforeach()
add_custom_target(translations ALL DEPENDS ${ENVIO_MO})
add_dependencies(envio translations)

install(TARGETS envio DESTINATION .)
install(IMPORTED_RUNTIME_ARTIFACTS envio-push-helper RUNTIME DESTINATION push)
install(DIRECTORY ${ENVIO_LOCALE_DIR} DESTINATION share)
install(FILES LICENSE NOTICES.md DESTINATION .)
# bindle (LGPL-3.0-only) is compiled in: ship its licence texts too.
install(FILES qml/bindle/COPYING qml/bindle/COPYING.LESSER DESTINATION licenses/bindle)
