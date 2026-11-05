Свой логотип и экран загрузки
=============================

Положи в эту папку (рядом с ProtoCore-MCS-Manager.exe):

  logo.png    — логотип (квадрат, 256x256 или больше). Также подходят .svg, .webp, .gif
  splash.gif  — анимация на экране загрузки. Также подходят .webp, .png

Перезапусти менеджер — файлы подхватятся без пересборки.

Иконка самого .exe задаётся при сборке: замени branding/icon-source.png в исходниках
и выполни  npx tauri icon branding/icon-source.png -o src-tauri/icons
