Feature: Changing the profile
  A signed-in user can change their name and their email address. A new email
  address only replaces the current one once its owner confirmed it, and the
  answer never reveals whether an address is already registered.

  Rule: A user can change their name

    Background:
      Given the app lets users sign in with a password or a passkey
      And I have signed up with the email "john@doe.name" and a password

    Scenario: A user changes their first and last name
      When I change my first name to "John" and my last name to "Doe"
      Then the app answers 200
      And the response is my account, named "John" "Doe", with the same email address
      And my account is named "John" "Doe"

    Scenario: A name that is too long is refused
      When I change my first name to a name that is one character too long
      Then the app answers 400
      And my account still has no first name

    Scenario: A change to a field that does not exist is refused
      When I send the profile change:
        """
        { "first_name": "John", "is_admin": true }
        """
      Then the app answers 400

    Scenario: A change that changes nothing finds nothing to update
      When I change nothing in my profile
      Then the app answers 404

    Scenario: A visitor who is not signed in cannot change a profile
      When a visitor who is not signed in changes their first name to "John"
      Then the app answers 401

  Rule: A new email address replaces the current one once it is confirmed

    Background:
      Given the app lets users sign in with a password or a passkey
      And I have signed up with the email "john@doe.name" and a password

    Scenario: A user with a verified address changes it
      When I open the link of the verification email
      Then my email address is verified
      When I request the change of my email address to "new@email.name"
      Then the app answers 200
      And the response still shows my current email address
      And that address is still verified
      And my current email address was told that a change to "n***@email.name" was requested
      When I open the link of the email change confirmation
      Then the app answers 200
      And my account, as the app now shows it, uses the verified email "new@email.name"
      And no email change is pending

    Scenario: Asking for the address the user already has changes nothing
      When I request the change of my email address to "john@doe.name"
      Then the app answers 304

    Scenario: An address that belongs to someone else is answered like a free one but cannot be confirmed
      Whether an email address is registered must not be observable.

      Given another user signed up with the email "jane@doe.name"
      When I request the change of my email address to "jane@doe.name"
      Then the app answers 200
      And the response still shows my current email address
      When I open the link of the email change confirmation
      Then the app answers 400
      And my account, as the app now shows it, uses the email "john@doe.name"

  Rule: An address of a refused domain cannot be used

    Scenario: A refused change leaves the account untouched
      Given the app lets users sign in with single sign-on, a password and a passkey
      And the app refuses registrations from the domain blocked.com
      And I have signed up with the email "john@allowed.com" and a password
      When I request the change of my email address to "john@blocked.com"
      Then the app answers 403
      And the error message is "Forbidden access: Registration is not allowed from this domain"
      And my account is exactly as it was when I signed up
