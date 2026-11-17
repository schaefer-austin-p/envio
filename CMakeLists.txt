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

find_package(Qt5 5.15 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2)

include(FetchContent)
FetchContent_Declare(CxxQt GIT_REPOSITORY https://github.com/kdab/cxx-qt-cmake.git GIT_TAG 0.10.0)
FetchContent_MakeAvailable(CxxQt)

cxx_qt_import_crate(MANIFEST_PATH qt/Cargo.toml CRATES envio_qt LOCKED
  QT_MODULES Qt5::Core Qt5::Gui Qt5::Qml)
cxx_qt_import_qml_module(envio_qml URI "Envio" SOURCE_CRATE envio_qt)

add_executable(envio qt/main.cpp qml/qml.qrc qml/bindle/bindle.qrc)
target_link_libraries(envio PRIVATE envio_qml Qt5::Core Qt5::Gui Qt5::Qml Qt5::Quick Qt5::QuickControls2)
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
  target_link_libraries(envio-qmltest PRIVATE envio_qml Qt5::Core Qt5::Gui Qt5::Qml Qt5::Quick
    Qt5::QuickTest Qt5::Network)
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
install(DIRECTORY ${ENVIO_LOCALE_DIR} DESTINATION share)
install(FILES LICENSE NOTICES.md DESTINATION .)
# bindle (LGPL-3.0-only) is compiled in: ship its licence texts too.
install(FILES qml/bindle/COPYING qml/bindle/COPYING.LESSER DESTINATION licenses/bindle)
