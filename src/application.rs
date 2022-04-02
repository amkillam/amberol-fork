// SPDX-FileCopyrightText: 2022  Emmanuele Bassi
// SPDX-License-Identifier: GPL-3.0-or-later

use adw::subclass::prelude::*;
use glib::clone;
use gtk::{gio, glib, prelude::*, subclass::prelude::*};

use crate::{
    config::{APPLICATION_ID, VERSION},
    i18n::i18n,
    AmberolWindow,
};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct AmberolApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for AmberolApplication {
        const NAME: &'static str = "AmberolApplication";
        type Type = super::AmberolApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for AmberolApplication {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.setup_gactions();

            obj.set_accels_for_action("app.quit", &["<primary>q"]);

            obj.set_accels_for_action("queue.add-song", &["s"]);
            obj.set_accels_for_action("queue.add-folder", &["a"]);
            obj.set_accels_for_action("queue.clear", &["<primary>L"]);
            obj.set_accels_for_action("win.previous", &["b"]);
            obj.set_accels_for_action("win.next", &["n"]);
            obj.set_accels_for_action("win.play", &["p"]);
        }
    }

    impl ApplicationImpl for AmberolApplication {
        fn activate(&self, application: &Self::Type) {
            debug!("AmberolApplication::activate");

            let window = if let Some(window) = application.active_window() {
                window
            } else {
                let window = AmberolWindow::new(application);
                window.upcast()
            };

            window.present();
        }
    }

    impl GtkApplicationImpl for AmberolApplication {}
    impl AdwApplicationImpl for AmberolApplication {}
}

glib::wrapper! {
    pub struct AmberolApplication(ObjectSubclass<imp::AmberolApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl AmberolApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::new(&[
            ("application-id", &application_id),
            ("flags", flags),
            // We don't change the resource path depending on the
            // profile, so we need to specify the base path ourselves
            ("resource-base-path", &"/io/bassi/Amberol"),
        ])
        .expect("Failed to create AmberolApplication")
    }

    fn setup_gactions(&self) {
        let quit_action = gio::SimpleAction::new("quit", None);
        quit_action.connect_activate(clone!(@weak self as app => move |_, _| {
            app.quit();
        }));
        self.add_action(&quit_action);

        let about_action = gio::SimpleAction::new("about", None);
        about_action.connect_activate(clone!(@weak self as app => move |_, _| {
            app.show_about();
        }));
        self.add_action(&about_action);
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let dialog = gtk::AboutDialog::builder()
            .transient_for(&window)
            .modal(true)
            .icon_name(APPLICATION_ID)
            .program_name("Amberol")
            .comments(&i18n("Plays music and nothing else"))
            .version(VERSION)
            .authors(vec!["Emmanuele Bassi".into()])
            .copyright("© 2022 Emmanuele Bassi")
            .license_type(gtk::License::Gpl30)
            // Translators: Replace "translator-credits" with your names, one name per line
            .translator_credits(&i18n("translator-credits"))
            .build();

        dialog.present();
    }
}
