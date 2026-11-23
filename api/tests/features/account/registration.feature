Feature: Registering an account
  A visitor registers with an email address and a password. Whether or not
  the address is already registered, the answer is the same, so nobody can
  find out who has an account. Passwords must be long enough, not too common
  and not too long.

  Rule: Registering creates an account that has to be verified

    Background:
      Given the app lets users sign in with a password or a passkey

    @legacy:test_users::register_user::test_register_user
    Scenario: A new user gets an unverified account and a verification email
      When I sign up with the email "john@doe.name" and the password "Very-harD-pasSword-5"
      Then my account uses the email "john@doe.name"
      And my email address is not verified yet and a verification email was sent
      And I have no API tokens
      And a verification token was issued for my account
      And 1 email was sent in total
      And the latest email sent is the verification email with my current link

    @legacy:test_users::register_user::test_register_existing_user_does_not_leak_email
    Scenario: Registering an address that is already registered does not reveal it
      The owner is told about the attempt by email instead.

      Given I have signed up with the email "john@doe.name" and a password
      When I sign up again with the email "john@doe.name" and the password "Very-harD-pasSword-5"
      Then the app answers 200
      And the app says "If this email is not already registered, you will receive a verification email shortly."
      And the owner of the account was told about the registration attempt

    @legacy:test_users::register_user::test_register_new_and_existing_user_return_same_response
    Scenario: A new and an already registered address get the same answer
      When a visitor signs up with the email "new@user.name" and the password "Very-harD-pasSword-5"
      And a visitor signs up with the email "new@user.name" and the password "Very-harD-pasSword-5"
      Then the app answered 200 every time
      And both answers carry the same message

  Rule: Passwords must be long enough, uncommon and not too long

    Background:
      Given the app lets users sign in with a password or a passkey

    @legacy:test_users::password_policy::test_register_user_rejects_weak_password
    Scenario Outline: A password that is <reason> is refused at registration
      When a visitor signs up with the email "weak@doe.name" and the password "<password>"
      Then the app answers 400

      Examples:
        | reason                           | password                                                                                                                          |
        | empty                            |                                                                                                                                   |
        | too short                        | abc                                                                                                                               |
        | one character short              | Kp9!vQ2#mX4                                                                                                                       |
        | too long                         | Kp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#m |
        | a common password                | 1qaz2wsx3edc                                                                                                                      |
        | a common password in other cases | 1QAZ2wsx3EDC                                                                                                                      |

    @legacy:test_users::password_policy::test_register_user_accepts_password_within_bounds
    Scenario Outline: A password of the <bound> length is accepted and can be used to sign in
      When a visitor signs up with the email "bounds@doe.name" and the password "<password>"
      Then the app answers 200
      When a visitor signs in with the email "bounds@doe.name" and the password "<password>"
      Then the app answers 200

      Examples:
        | bound   | password                                                                                                                         |
        | minimum | Kp9!vQ2#mX4z                                                                                                                     |
        | maximum | Kp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2#mX4zKp9!vQ2# |

    @legacy:test_users::password_policy::test_login_with_legacy_short_password
    Scenario: An account created under an older, looser policy can still sign in
      Given an account was created with the old password "short1" and the email "legacy@doe.name"
      When a visitor signs in with the email "legacy@doe.name" and the password "short1"
      Then the app answers 200

    @legacy:test_users::password_policy::test_login_with_too_long_password
    Scenario: A password that is too long is refused at sign-in like a wrong one
      Given I have signed up with the email "john@doe.name" and a password
      When a visitor signs in with the email "john@doe.name" and the password "Very-harD-pasSword-5Very-harD-pasSword-5Very-harD-pasSword-5Very-harD-pasSword-5Very-harD-pasSword-5Very-harD-pasSword-5Very-harD-pasSword-5"
      Then the app answers 401
      And the error message is "Unauthorized access: Invalid email address or password"

    @legacy:test_users::password_policy::test_reset_password_rejects_weak_password
    Scenario Outline: A new password that is <reason> is refused when resetting a password
      Given I have signed up with the email "john@doe.name" and a password
      When I ask for my password to be reset
      Then the app answers 200
      When I reset my password to "<password>" with the link in the email
      Then the app answers 400

      Examples:
        | reason            | password     |
        | empty             |              |
        | a common password | 1qaz2wsx3edc |

  Rule: Some email domains cannot register

    Background:
      Given the app lets users sign in with single sign-on, a password or a passkey, and refuses the domain blocked.com

    @legacy:test_users::email_domain_blacklist::test_register_user_with_blacklisted_domain
    Scenario: A visitor cannot register with an address of a refused domain
      When a visitor signs up with the email "user@blocked.com" and the password "Very-harD-pasSword-5"
      Then the app answers 403
      And the error message is "Forbidden access: Registration is not allowed from this domain"

    @legacy:test_users::email_domain_blacklist::test_register_user_with_allowed_domain
    Scenario: A visitor can register with an address of another domain
      When I sign up with the email "user@allowed.com" and the password "Very-harD-pasSword-5"
      Then my account uses the email "user@allowed.com"

    @legacy:test_users::email_domain_blacklist::test_register_user_blacklist_case_insensitive
    Scenario: Refusing a domain ignores the case of the address
      When a visitor signs up with the email "user@BLOCKED.COM" and the password "Very-harD-pasSword-5"
      Then the app answers 403
      And the error message is "Forbidden access: Registration is not allowed from this domain"

    @legacy:test_users::email_domain_blacklist::test_oidc_authenticate_with_blacklisted_domain
    Scenario: A user cannot sign in with single sign-on with an address of a refused domain
      Given the identity provider knows me as "John" "Doe" with the email "user@blocked.com"
      When I sign in with a fresh identity token for "user@blocked.com"
      Then the app answers 403
      And the error message is "Forbidden access: Registration is not allowed from this domain"
